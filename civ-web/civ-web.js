import init, {
    FrameBuffer,
    encode_read_frequency,
    encode_read_mode,
    encode_set_frequency,
    encode_set_mode,
    encode_select_vfo,
    encode_power_on,
    encode_power_off,
    encode_read_level,
    encode_read_s_meter,
    encode_read_gps,
    encode_set_level,
    encode_read_tone_mode,
    encode_set_tone_mode,
    encode_read_tx_tone,
    encode_read_rx_tone,
    encode_set_tx_tone,
    encode_set_rx_tone,
    encode_read_dtcs,
    encode_set_dtcs,
} from "./pkg/civ_web.js";

// CI-V level sub-command constants.
const LEVEL_AF = 0x01;
const LEVEL_SQUELCH = 0x03;

// CI-V various sub-command constants.
const VARIOUS_TONE_SQUELCH_FUNC = 0x5d;

// Tone mode labels (0x00–0x03 are what the ID-52A supports).
const TONE_MODE_LABELS = ["CSQ", "Tone", "TSQL", "DTCS"];

let port = null;
let reader = null;
let frameBuffer = null;
let pollTimer = null;
let gpsPollTimer = null;
let currentVfo = "A";
let radioVfo = "A";
let disconnecting = false;

// ── Command Queue ───────────────────────────────────────────────────────────
// Sends one command at a time, waits for response before sending the next.

const cmdQueue = [];
let cmdInFlight = null; // { resolve, reject, timeout, vfo }
const CMD_TIMEOUT_MS = 2000;

// Enqueue a command. Returns a promise that resolves with the parsed response.
function sendCommand(bytes, { vfo = null } = {}) {
    if (disconnecting) return Promise.resolve(null);
    return new Promise((resolve, reject) => {
        cmdQueue.push({ bytes, resolve, reject, vfo });
        pumpQueue();
    });
}

function pumpQueue() {
    if (disconnecting || cmdInFlight || cmdQueue.length === 0) return;
    if (!port?.writable) {
        // Drain queue if disconnected.
        while (cmdQueue.length > 0) {
            cmdQueue.shift().resolve(null);
        }
        return;
    }

    const cmd = cmdQueue.shift();
    cmdInFlight = cmd;

    // Set a timeout so we don't hang forever if the radio doesn't respond.
    cmd.timeout = setTimeout(() => {
        if (cmdInFlight === cmd) {
            cmdInFlight = null;
            cmd.resolve(null); // Resolve with null rather than rejecting — keeps polling alive.
            pumpQueue();
        }
    }, CMD_TIMEOUT_MS);

    // Write bytes to serial.
    const writer = port.writable.getWriter();
    logTx(cmd.bytes);
    writer.write(new Uint8Array(cmd.bytes)).then(() => {
        writer.releaseLock();
    }).catch((err) => {
        writer.releaseLock();
        clearTimeout(cmd.timeout);
        cmdInFlight = null;
        cmd.reject(err);
        pumpQueue();
    });
}

// Called by the read loop when a response is parsed.
function resolveInFlight(resp) {
    if (!cmdInFlight) return;
    clearTimeout(cmdInFlight.timeout);
    const cmd = cmdInFlight;
    cmdInFlight = null;
    if (disconnecting) {
        cmd.resolve(null);
        return;
    }
    cmd.resolve({ ...resp, vfo: cmd.vfo });
    pumpQueue();
}

// ── DOM references ──────────────────────────────────────────────────────────

const btnConnect = document.getElementById("btn-connect");
const btnDisconnect = document.getElementById("btn-disconnect");
const statusEl = document.getElementById("status");

const freqA = document.getElementById("freq-a");
const freqB = document.getElementById("freq-b");
const modeA = document.getElementById("mode-a");
const modeB = document.getElementById("mode-b");
const vfoAPanel = document.getElementById("vfo-a");
const vfoBPanel = document.getElementById("vfo-b");

const freqInput = document.getElementById("freq-input");
const btnSetFreq = document.getElementById("btn-set-freq");

const sMeterFill = document.getElementById("s-meter-fill");
const sMeterValue = document.getElementById("s-meter-value");
const volFill = document.getElementById("vol-fill");
const volValue = document.getElementById("vol-value");
const sqlFill = document.getElementById("sql-fill");
const sqlValue = document.getElementById("sql-value");

const toneA = document.getElementById("tone-a");
const toneB = document.getElementById("tone-b");

const toneFreqSelect = document.getElementById("tone-freq-select");
const btnSetTone = document.getElementById("btn-set-tone");
const toneFreqGroup = document.getElementById("tone-freq-group");
const dtcsGroup = document.getElementById("dtcs-group");
const dtcsCodeSelect = document.getElementById("dtcs-code-select");
const dtcsPolaritySelect = document.getElementById("dtcs-polarity");
const btnSetDtcs = document.getElementById("btn-set-dtcs");

const gpsData = document.getElementById("gps-data");
const frameLog = document.getElementById("frame-log");

// ── Per-VFO tone state ──────────────────────────────────────────────────────

const vfoToneState = {
    A: { mode: null, txTone: null, rxTone: null, dtcsCode: null, dtcsTxPol: null, dtcsRxPol: null },
    B: { mode: null, txTone: null, rxTone: null, dtcsCode: null, dtcsTxPol: null, dtcsRxPol: null },
};

// ── Logging ─────────────────────────────────────────────────────────────────

function log(msg, cls = "log-rx") {
    const entry = document.createElement("div");
    entry.className = `log-entry ${cls}`;
    const ts = new Date().toLocaleTimeString();
    entry.textContent = `[${ts}] ${msg}`;
    frameLog.prepend(entry);

    // Keep log manageable.
    while (frameLog.children.length > 200) {
        frameLog.removeChild(frameLog.lastChild);
    }
}

// Map a byte value (0–255) to an HSL color.
// 0x00 = cool blue, 0xFF = hot red, mid-range = green/yellow.
function byteColor(b) {
    // Hue: 240 (blue) → 0 (red), saturation 80%, lightness 55%.
    const hue = 240 - (b / 255) * 240;
    return `hsl(${hue}, 80%, 55%)`;
}

function logHex(direction, bytes, cls) {
    const entry = document.createElement("div");
    entry.className = `log-entry ${cls}`;
    const ts = new Date().toLocaleTimeString();

    const prefix = document.createElement("span");
    prefix.textContent = `[${ts}] ${direction}: `;
    entry.appendChild(prefix);

    for (let i = 0; i < bytes.length; i++) {
        if (i > 0) entry.appendChild(document.createTextNode(" "));
        const span = document.createElement("span");
        span.className = "hex-byte";
        span.textContent = bytes[i].toString(16).padStart(2, "0").toUpperCase();
        span.style.color = byteColor(bytes[i]);
        entry.appendChild(span);
    }

    frameLog.prepend(entry);
    while (frameLog.children.length > 200) {
        frameLog.removeChild(frameLog.lastChild);
    }
}

function logTx(bytes) {
    logHex("TX", Array.from(bytes), "log-tx");
}

// ── Serial I/O ──────────────────────────────────────────────────────────────

async function connect() {
    try {
        port = await navigator.serial.requestPort();
        await port.open({ baudRate: 19200 });
        setConnected(true);
        frameBuffer = new FrameBuffer();
        startReading();
        // Wake the radio if it's asleep.
        await sendCommand(encode_power_on());
        // Initial reads — query both VFOs.
        await selectVfo("A");
        await readVfoState("A");
        await selectVfo("B");
        await readVfoState("B");
        // Switch back to A as the active VFO.
        await selectVfo("A");
        currentVfo = "A";
        updateVfoHighlight();
        // Start polling after initial state is queried.
        startPolling();
    } catch (err) {
        log(`Connect error: ${err.message}`, "log-err");
    }
}

async function selectVfo(vfo) {
    radioVfo = vfo;
    const resp = await sendCommand(encode_select_vfo(vfo));
    if (resp) handleResponse(resp);
}

async function readVfoState(vfo) {
    const freqResp = await sendCommand(encode_read_frequency(), { vfo });
    if (freqResp) handleResponse(freqResp);
    const modeResp = await sendCommand(encode_read_mode(), { vfo });
    if (modeResp) handleResponse(modeResp);
    await readVfoToneState(vfo);
}

async function readVfoToneState(vfo) {
    const tmResp = await sendCommand(encode_read_tone_mode(), { vfo });
    if (tmResp) handleResponse(tmResp);
    const txResp = await sendCommand(encode_read_tx_tone(), { vfo });
    if (txResp) handleResponse(txResp);
    const rxResp = await sendCommand(encode_read_rx_tone(), { vfo });
    if (rxResp) handleResponse(rxResp);
    const dtcsResp = await sendCommand(encode_read_dtcs(), { vfo });
    if (dtcsResp) handleResponse(dtcsResp);
}

async function disconnect() {
    if (disconnecting) return;
    disconnecting = true;

    stopPolling();

    // Drain any pending commands.
    while (cmdQueue.length > 0) {
        cmdQueue.shift().resolve(null);
    }
    if (cmdInFlight) {
        clearTimeout(cmdInFlight.timeout);
        const cmd = cmdInFlight;
        cmdInFlight = null;
        cmd.resolve(null);
    }

    // Cancel the reader — this causes reader.read() to resolve with {done: true},
    // which lets startReading() exit its loop and call releaseLock().
    if (reader) {
        try { await reader.cancel(); } catch (_) {}
        // reader is set to null in startReading()'s finally block.
    }

    if (port) {
        try { await port.close(); } catch (_) {}
        port = null;
    }

    setConnected(false);
    if (frameBuffer) {
        frameBuffer.free();
        frameBuffer = null;
    }

    disconnecting = false;
}

function setConnected(connected) {
    btnConnect.disabled = connected;
    btnDisconnect.disabled = !connected;
    statusEl.textContent = connected ? "Connected" : "Disconnected";
    statusEl.className = connected ? "connected" : "";
}

async function startReading() {
    while (port?.readable && !disconnecting) {
        reader = port.readable.getReader();
        try {
            while (true) {
                const { value, done } = await reader.read();
                if (done) break;
                if (value && frameBuffer && !disconnecting) {
                    logHex("RX", Array.from(value), "log-rx");
                    try {
                        const responses = frameBuffer.feed(value);
                        for (const resp of responses) {
                            resolveInFlight(resp);
                        }
                    } catch (err) {
                        log(`Parse error: ${err}`, "log-err");
                        // Resolve in-flight so the queue doesn't stall.
                        resolveInFlight(null);
                    }
                }
            }
        } catch (err) {
            if (err.name !== "NetworkError") {
                log(`Read error: ${err.message}`, "log-err");
            }
        } finally {
            reader.releaseLock();
            reader = null;
        }
    }
}

// ── Polling ─────────────────────────────────────────────────────────────────

function startPolling() {
    pollTimer = setInterval(async () => {
        if (!port?.writable) return;
        // Skip this poll cycle if the queue is backed up.
        if (cmdQueue.length > 0 || cmdInFlight) return;
        try {
            const sResp = await sendCommand(encode_read_s_meter());
            if (sResp) handleResponse(sResp);
            const afResp = await sendCommand(encode_read_level(LEVEL_AF));
            if (afResp) handleResponse(afResp);
            const sqlResp = await sendCommand(encode_read_level(LEVEL_SQUELCH));
            if (sqlResp) handleResponse(sqlResp);
        } catch (_) {}
    }, 500);

    gpsPollTimer = setInterval(async () => {
        if (!port?.writable) return;
        if (cmdQueue.length > 0 || cmdInFlight) return;
        try {
            const gpsResp = await sendCommand(encode_read_gps());
            if (gpsResp) handleResponse(gpsResp);
        } catch (_) {}
    }, 5000);
}

function stopPolling() {
    if (pollTimer) {
        clearInterval(pollTimer);
        pollTimer = null;
    }
    if (gpsPollTimer) {
        clearInterval(gpsPollTimer);
        gpsPollTimer = null;
    }
}

// ── Response handling ───────────────────────────────────────────────────────

function handleResponse(resp) {
    if (!resp || !resp.type) return;

    switch (resp.type) {
        case "ok":
            log("OK");
            break;
        case "ng":
            log("NG (command rejected)", "log-err");
            break;
        case "frequency":
            updateFrequency(resp);
            break;
        case "mode":
            updateMode(resp);
            break;
        case "level":
            updateLevel(resp.sub, resp.value);
            break;
        case "meter":
            updateMeter(resp.sub, resp.value);
            break;
        case "gps":
            updateGps(resp);
            break;
        case "various":
            updateVarious(resp);
            break;
        case "tone_frequency":
            updateToneFrequency(resp);
            break;
        case "dtcs":
            updateDtcs(resp);
            break;
        case "duplex":
            log(`Duplex: ${resp.direction}`);
            break;
        case "offset":
            log(`Offset: ${resp.display}`);
            break;
        case "transceiver_id":
            log(`Transceiver ID: 0x${resp.id.toString(16).toUpperCase()}`);
            break;
        default:
            log(`Response: ${JSON.stringify(resp)}`);
    }
}

function updateFrequency(resp) {
    const vfo = resp.vfo || radioVfo;
    if (vfo === "A") {
        freqA.textContent = resp.display;
    } else {
        freqB.textContent = resp.display;
    }
}

function updateMode(resp) {
    const vfo = resp.vfo || radioVfo;
    if (vfo === "A") {
        modeA.textContent = resp.mode;
    } else {
        modeB.textContent = resp.mode;
    }
    // Highlight active mode button for the current UI VFO.
    if (vfo === currentVfo) {
        document.querySelectorAll(".mode-btn").forEach((btn) => {
            btn.classList.toggle("active", btn.dataset.mode === resp.mode);
        });
    }
}

function updateLevel(sub, value) {
    const pct = Math.min(100, (value / 255) * 100);
    if (sub === LEVEL_AF) {
        volFill.style.width = `${pct}%`;
        volValue.textContent = value;
    } else if (sub === LEVEL_SQUELCH) {
        sqlFill.style.width = `${pct}%`;
        sqlValue.textContent = value;
    }
}

function updateMeter(sub, value) {
    if (sub === 0x02) {
        const pct = Math.min(100, (value / 255) * 100);
        sMeterFill.style.width = `${pct}%`;
        sMeterValue.textContent = value;
    }
}

function updateGps(data) {
    const lat = data.latitude.toFixed(6);
    const lon = data.longitude.toFixed(6);
    const alt = data.altitude_m.toFixed(1);
    const spd = data.speed_kmh.toFixed(1);
    gpsData.textContent = `${lat}, ${lon} | Alt: ${alt}m | Speed: ${spd} km/h | Course: ${data.course}°`;
}

function updateVarious(resp) {
    const vfo = resp.vfo || radioVfo;
    if (resp.sub === VARIOUS_TONE_SQUELCH_FUNC) {
        vfoToneState[vfo].mode = resp.value;
        refreshToneDisplay(vfo);
        // Update tone mode buttons if this is the active VFO.
        if (vfo === currentVfo) {
            document.querySelectorAll(".tone-mode-btn").forEach((btn) => {
                btn.classList.toggle("active", parseInt(btn.dataset.toneMode) === resp.value);
            });
            updateToneControlVisibility(resp.value);
        }
    }
}

function updateToneFrequency(resp) {
    const vfo = resp.vfo || radioVfo;
    if (resp.sub === 0x00) {
        vfoToneState[vfo].txTone = resp.tenths_hz;
    } else if (resp.sub === 0x01) {
        vfoToneState[vfo].rxTone = resp.tenths_hz;
    }
    refreshToneDisplay(vfo);
    // Update the tone frequency dropdown if this is the active VFO.
    if (vfo === currentVfo) {
        const freq = resp.sub === 0x00 ? resp.tenths_hz : resp.tenths_hz;
        toneFreqSelect.value = String(freq);
    }
}

function updateDtcs(resp) {
    const vfo = resp.vfo || radioVfo;
    vfoToneState[vfo].dtcsCode = resp.code;
    vfoToneState[vfo].dtcsTxPol = resp.tx_polarity;
    vfoToneState[vfo].dtcsRxPol = resp.rx_polarity;
    refreshToneDisplay(vfo);
    // Update DTCS controls if this is the active VFO.
    if (vfo === currentVfo) {
        dtcsCodeSelect.value = String(resp.code);
        const polStr =
            (resp.tx_polarity ? "R" : "N") +
            (resp.rx_polarity ? "R" : "N");
        dtcsPolaritySelect.value = polStr;
    }
}

function formatToneFreq(tenths) {
    if (tenths == null) return null;
    return (tenths / 10).toFixed(1);
}

function refreshToneDisplay(vfo) {
    const state = vfoToneState[vfo];
    const el = vfo === "A" ? toneA : toneB;
    const mode = state.mode;

    if (mode == null) {
        el.textContent = "--";
        return;
    }

    const label = TONE_MODE_LABELS[mode] || `T${mode}`;

    switch (mode) {
        case 0: // CSQ — no tone
            el.textContent = "CSQ";
            break;
        case 1: // Tone (Tx only)
            el.textContent = state.txTone != null
                ? `T ${formatToneFreq(state.txTone)}`
                : "Tone";
            break;
        case 2: // TSQL
            el.textContent = state.rxTone != null
                ? `TSQL ${formatToneFreq(state.rxTone)}`
                : "TSQL";
            break;
        case 3: { // DTCS
            const code = state.dtcsCode != null
                ? String(state.dtcsCode).padStart(3, "0")
                : "---";
            const pol = state.dtcsTxPol != null
                ? (state.dtcsTxPol ? "R" : "N") + (state.dtcsRxPol ? "R" : "N")
                : "";
            el.textContent = `DCS ${code} ${pol}`.trim();
            break;
        }
        default:
            el.textContent = label;
    }
}

function updateToneControlVisibility(mode) {
    // Show tone freq controls for Tone and TSQL modes, DTCS controls for DTCS mode.
    toneFreqGroup.style.display = (mode === 1 || mode === 2) ? "flex" : "none";
    dtcsGroup.style.display = (mode === 3) ? "flex" : "none";
}

function updateVfoHighlight() {
    vfoAPanel.classList.toggle("active", currentVfo === "A");
    vfoBPanel.classList.toggle("active", currentVfo === "B");
}

// ── Event handlers ──────────────────────────────────────────────────────────

btnConnect.addEventListener("click", connect);
btnDisconnect.addEventListener("click", disconnect);

// VFO select buttons.
document.querySelectorAll(".vfo-select").forEach((btn) => {
    btn.addEventListener("click", async () => {
        const vfo = btn.dataset.vfo;
        currentVfo = vfo;
        updateVfoHighlight();
        syncToneControlsToVfo(vfo);
        await selectVfo(vfo);
        await readVfoState(vfo);
    });
});

// Set frequency.
btnSetFreq.addEventListener("click", async () => {
    const mhz = parseFloat(freqInput.value);
    if (isNaN(mhz) || mhz <= 0) {
        log("Invalid frequency", "log-err");
        return;
    }
    const hz = Math.round(mhz * 1_000_000);
    const setResp = await sendCommand(encode_set_frequency(hz));
    if (setResp) handleResponse(setResp);
    // Read back to confirm.
    const readResp = await sendCommand(encode_read_frequency(), { vfo: radioVfo });
    if (readResp) handleResponse(readResp);
});

freqInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") btnSetFreq.click();
});

// Mode buttons.
document.querySelectorAll(".mode-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
        const setResp = await sendCommand(encode_set_mode(btn.dataset.mode));
        if (setResp) handleResponse(setResp);
        const readResp = await sendCommand(encode_read_mode(), { vfo: radioVfo });
        if (readResp) handleResponse(readResp);
    });
});

// Power buttons.
document.getElementById("btn-power-on").addEventListener("click", async () => {
    const resp = await sendCommand(encode_power_on());
    if (resp) handleResponse(resp);
    // Re-query VFO state and resume polling.
    await selectVfo("A");
    await readVfoState("A");
    await selectVfo("B");
    await readVfoState("B");
    await selectVfo(currentVfo);
    startPolling();
});

document.getElementById("btn-power-off").addEventListener("click", async () => {
    stopPolling();
    const resp = await sendCommand(encode_power_off());
    if (resp) handleResponse(resp);
});

// Tone mode buttons.
document.querySelectorAll(".tone-mode-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
        const mode = parseInt(btn.dataset.toneMode);
        const setResp = await sendCommand(encode_set_tone_mode(mode));
        if (setResp) handleResponse(setResp);
        // Read back to confirm.
        const readResp = await sendCommand(encode_read_tone_mode(), { vfo: radioVfo });
        if (readResp) handleResponse(readResp);
    });
});

// Set tone frequency (applies to both Tx and Rx tone for simplicity).
btnSetTone.addEventListener("click", async () => {
    const freqTenths = parseInt(toneFreqSelect.value);
    const state = vfoToneState[currentVfo];
    const mode = state.mode;

    if (mode === 1) {
        // Tone mode — set Tx tone.
        const resp = await sendCommand(encode_set_tx_tone(freqTenths));
        if (resp) handleResponse(resp);
        const readResp = await sendCommand(encode_read_tx_tone(), { vfo: radioVfo });
        if (readResp) handleResponse(readResp);
    } else if (mode === 2) {
        // TSQL — set both Tx and Rx to the same frequency.
        const txResp = await sendCommand(encode_set_tx_tone(freqTenths));
        if (txResp) handleResponse(txResp);
        const rxResp = await sendCommand(encode_set_rx_tone(freqTenths));
        if (rxResp) handleResponse(rxResp);
        const readTx = await sendCommand(encode_read_tx_tone(), { vfo: radioVfo });
        if (readTx) handleResponse(readTx);
        const readRx = await sendCommand(encode_read_rx_tone(), { vfo: radioVfo });
        if (readRx) handleResponse(readRx);
    }
});

// Set DTCS code.
btnSetDtcs.addEventListener("click", async () => {
    const code = parseInt(dtcsCodeSelect.value);
    const polStr = dtcsPolaritySelect.value;
    const txPol = polStr[0] === "R" ? 1 : 0;
    const rxPol = polStr[1] === "R" ? 1 : 0;
    const setResp = await sendCommand(encode_set_dtcs(txPol, rxPol, code));
    if (setResp) handleResponse(setResp);
    const readResp = await sendCommand(encode_read_dtcs(), { vfo: radioVfo });
    if (readResp) handleResponse(readResp);
});

// Sync tone controls (buttons, dropdowns) to reflect a VFO's cached state.
function syncToneControlsToVfo(vfo) {
    const state = vfoToneState[vfo];
    // Tone mode buttons.
    document.querySelectorAll(".tone-mode-btn").forEach((btn) => {
        btn.classList.toggle("active", parseInt(btn.dataset.toneMode) === state.mode);
    });
    // Tone frequency dropdown.
    if (state.txTone != null) toneFreqSelect.value = String(state.txTone);
    // DTCS controls.
    if (state.dtcsCode != null) dtcsCodeSelect.value = String(state.dtcsCode);
    if (state.dtcsTxPol != null) {
        dtcsPolaritySelect.value =
            (state.dtcsTxPol ? "R" : "N") + (state.dtcsRxPol ? "R" : "N");
    }
    updateToneControlVisibility(state.mode ?? 0);
}

// ── Initialize WASM ─────────────────────────────────────────────────────────

async function main() {
    await init();
    log("WASM initialized");

    if (!("serial" in navigator)) {
        log("WebSerial not supported in this browser. Use Chrome or Edge.", "log-err");
        btnConnect.disabled = true;
    }
}

main();
