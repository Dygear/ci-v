use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tokio::sync::mpsc as tokio_mpsc;

use civ_protocol::Radio;

use crate::message::{RadioCommand, RadioEvent, RadioState, Vfo, VfoState};

/// Bits per byte on the wire with 8N1 framing (1 start + 8 data + 1 stop).
const BITS_PER_BYTE: u64 = 10;

/// Polling intervals per tier.
const FAST_INTERVAL: Duration = Duration::from_millis(200);
const MEDIUM_INTERVAL: Duration = Duration::from_millis(1_000);
const SLOW_INTERVAL: Duration = Duration::from_millis(5_000);

/// Run the radio polling loop on a blocking thread.
///
/// Polls radio state in three tiers:
///   Fast  (200 ms) — frequency, mode, S-meter.
///   Medium  (1 s)  — AF level, squelch, RF power, duplex, offset.
///   Slow    (5 s)  — tone mode, Tx/Rx tone, DTCS, GPS.
///
/// State is only sent to the TUI when a value actually changes, preventing
/// unnecessary redraws.
pub fn radio_loop(
    mut radio: Radio,
    cmd_rx: std_mpsc::Receiver<RadioCommand>,
    event_tx: tokio_mpsc::UnboundedSender<RadioEvent>,
) {
    let _ = event_tx.send(RadioEvent::Connected);

    // Power on the radio (harmless if already on).
    if let Err(e) = radio.power_on() {
        let _ = event_tx.send(RadioEvent::Error(format!("power on: {e}")));
    }
    // Allow the radio time to boot before polling.
    thread::sleep(Duration::from_millis(500));

    // Initialization: read full state for both VFOs.
    let mut active_vfo = Vfo::A;

    let _ = radio.select_vfo_a();
    let vfo_a = poll_all_vfo_fields(&mut radio);

    let _ = radio.select_vfo_b();
    let vfo_b = poll_all_vfo_fields(&mut radio);

    let _ = radio.select_vfo_a();

    let mut state = RadioState {
        vfo_a,
        vfo_b,
        s_meter: radio.read_s_meter().ok(),
        af_level: radio.read_af_level().ok(),
        squelch: radio.read_squelch().ok(),
        gps_position: radio.read_gps_position().ok(),
        tx_bits_per_sec: 0,
        rx_bits_per_sec: 0,
    };

    // Send the initial state to the TUI.
    let _ = event_tx.send(RadioEvent::StateUpdate(state.clone()));

    let mut last_medium = Instant::now();
    let mut last_slow = Instant::now();
    let mut last_rate_time = Instant::now();
    let mut last_tx_bytes = radio.tx_bytes();
    let mut last_rx_bytes = radio.rx_bytes();

    loop {
        let cycle_start = Instant::now();

        // Process any pending commands (non-blocking).
        match cmd_rx.try_recv() {
            Ok(RadioCommand::Quit) => {
                let _ = event_tx.send(RadioEvent::Disconnected);
                return;
            }
            Ok(RadioCommand::SelectVfo(vfo)) => {
                active_vfo = vfo;
                if let Err(e) = execute_command(&mut radio, &RadioCommand::SelectVfo(vfo)) {
                    let _ = event_tx.send(RadioEvent::Error(format!("{e}")));
                }
            }
            Ok(cmd) => {
                if let Err(e) = execute_command(&mut radio, &cmd) {
                    let _ = event_tx.send(RadioEvent::Error(format!("{e}")));
                }
            }
            Err(std_mpsc::TryRecvError::Empty) => {}
            Err(std_mpsc::TryRecvError::Disconnected) => return,
        }

        let mut changed = false;

        // --- Fast tier: frequency, mode, S-meter ---
        let new_freq = radio.read_frequency().ok();
        let new_mode = radio.read_mode().ok();
        let new_s_meter = radio.read_s_meter().ok();

        {
            let vfo = active_vfo_mut(&mut state, active_vfo);
            if new_freq != vfo.frequency {
                vfo.frequency = new_freq;
                changed = true;
            }
            if new_mode != vfo.mode {
                vfo.mode = new_mode;
                changed = true;
            }
        }
        if new_s_meter != state.s_meter {
            state.s_meter = new_s_meter;
            changed = true;
        }

        // --- Medium tier: AF level, squelch, RF power, duplex, offset ---
        if last_medium.elapsed() >= MEDIUM_INTERVAL {
            let new_af = radio.read_af_level().ok();
            let new_sql = radio.read_squelch().ok();
            let new_rf = radio.read_rf_power().ok();
            let new_duplex = radio.read_duplex().ok();
            let new_offset = radio.read_offset().ok();

            if new_af != state.af_level {
                state.af_level = new_af;
                changed = true;
            }
            if new_sql != state.squelch {
                state.squelch = new_sql;
                changed = true;
            }
            {
                let vfo = active_vfo_mut(&mut state, active_vfo);
                if new_rf != vfo.rf_power {
                    vfo.rf_power = new_rf;
                    changed = true;
                }
                if new_duplex != vfo.duplex {
                    vfo.duplex = new_duplex;
                    changed = true;
                }
                if new_offset != vfo.offset {
                    vfo.offset = new_offset;
                    changed = true;
                }
            }

            last_medium = Instant::now();
        }

        // --- Slow tier: tone mode, Tx/Rx tone, DTCS, GPS ---
        if last_slow.elapsed() >= SLOW_INTERVAL {
            let new_tone_mode = radio.read_tone_mode().ok();
            let new_tx_tone = radio.read_tx_tone().ok();
            let new_rx_tone = radio.read_rx_tone().ok();
            let new_dtcs = radio.read_dtcs().ok();
            let new_gps = radio.read_gps_position().ok();

            {
                let vfo = active_vfo_mut(&mut state, active_vfo);
                if new_tone_mode != vfo.tone_mode {
                    vfo.tone_mode = new_tone_mode;
                    changed = true;
                }
                if new_tx_tone != vfo.tx_tone_freq {
                    vfo.tx_tone_freq = new_tx_tone;
                    changed = true;
                }
                if new_rx_tone != vfo.rx_tone_freq {
                    vfo.rx_tone_freq = new_rx_tone;
                    changed = true;
                }
                let new_dtcs_code = new_dtcs.map(|(_, _, c)| c);
                let new_dtcs_tx_pol = new_dtcs.map(|(tx, _, _)| tx);
                let new_dtcs_rx_pol = new_dtcs.map(|(_, rx, _)| rx);
                if new_dtcs_code != vfo.dtcs_code {
                    vfo.dtcs_code = new_dtcs_code;
                    changed = true;
                }
                if new_dtcs_tx_pol != vfo.dtcs_tx_pol {
                    vfo.dtcs_tx_pol = new_dtcs_tx_pol;
                    changed = true;
                }
                if new_dtcs_rx_pol != vfo.dtcs_rx_pol {
                    vfo.dtcs_rx_pol = new_dtcs_rx_pol;
                    changed = true;
                }
            }
            if new_gps != state.gps_position {
                state.gps_position = new_gps;
                changed = true;
            }

            last_slow = Instant::now();
        }

        // --- Bps counter update (every 1 s) ---
        let rate_elapsed = last_rate_time.elapsed().as_secs_f64();
        if rate_elapsed >= 1.0 {
            let tx_delta = radio.tx_bytes() - last_tx_bytes;
            let rx_delta = radio.rx_bytes() - last_rx_bytes;
            let new_tx_bps = (tx_delta as f64 * BITS_PER_BYTE as f64 / rate_elapsed).round() as u32;
            let new_rx_bps = (rx_delta as f64 * BITS_PER_BYTE as f64 / rate_elapsed).round() as u32;
            if new_tx_bps != state.tx_bits_per_sec || new_rx_bps != state.rx_bits_per_sec {
                state.tx_bits_per_sec = new_tx_bps;
                state.rx_bits_per_sec = new_rx_bps;
                changed = true;
            }
            last_tx_bytes = radio.tx_bytes();
            last_rx_bytes = radio.rx_bytes();
            last_rate_time = Instant::now();
        }

        // Only send a state update when something actually changed.
        if changed
            && event_tx
                .send(RadioEvent::StateUpdate(state.clone()))
                .is_err()
        {
            return;
        }

        // Sleep for the remainder of the fast-tier interval.
        let elapsed = cycle_start.elapsed();
        if elapsed < FAST_INTERVAL {
            thread::sleep(FAST_INTERVAL - elapsed);
        }
    }
}

/// Mutable reference to the active VFO within `state`.
fn active_vfo_mut<'a>(state: &'a mut RadioState, vfo: Vfo) -> &'a mut VfoState {
    match vfo {
        Vfo::A => &mut state.vfo_a,
        Vfo::B => &mut state.vfo_b,
    }
}

/// Read all VFO-specific fields in one pass (used during initialization).
fn poll_all_vfo_fields(radio: &mut Radio) -> VfoState {
    let frequency = radio.read_frequency().ok();
    let mode = radio.read_mode().ok();
    let rf_power = radio.read_rf_power().ok();
    let tone_mode = radio.read_tone_mode().ok();
    let duplex = radio.read_duplex().ok();
    let offset = radio.read_offset().ok();
    let tx_tone_freq = radio.read_tx_tone().ok();
    let rx_tone_freq = radio.read_rx_tone().ok();
    let dtcs = radio.read_dtcs().ok();

    VfoState {
        frequency,
        mode,
        rf_power,
        tone_mode,
        tx_tone_freq,
        rx_tone_freq,
        dtcs_code: dtcs.map(|(_, _, code)| code),
        dtcs_tx_pol: dtcs.map(|(tx, _, _)| tx),
        dtcs_rx_pol: dtcs.map(|(_, rx, _)| rx),
        duplex,
        offset,
    }
}

fn execute_command(radio: &mut Radio, cmd: &RadioCommand) -> civ_protocol::Result<()> {
    match cmd {
        RadioCommand::SetFrequency(freq) => radio.set_frequency(*freq),
        RadioCommand::SetMode(mode) => radio.set_mode(*mode),
        RadioCommand::SetAfLevel(level) => radio.set_af_level(*level),
        RadioCommand::SetSquelch(level) => radio.set_squelch(*level),
        RadioCommand::SelectVfo(vfo) => match vfo {
            Vfo::A => radio.select_vfo_a(),
            Vfo::B => radio.select_vfo_b(),
        },
        RadioCommand::SetRfPower(level) => radio.set_rf_power(*level),
        RadioCommand::SetDuplex(dir) => radio.set_duplex(*dir),
        RadioCommand::SetOffset(hz) => radio.set_offset(*hz),
        RadioCommand::SetToneMode(mode) => radio.set_tone_mode(*mode),
        RadioCommand::SetTxTone(freq) => radio.set_tx_tone(*freq),
        RadioCommand::SetRxTone(freq) => radio.set_rx_tone(*freq),
        RadioCommand::SetDtcsCode(tx_pol, rx_pol, code) => radio.set_dtcs(*tx_pol, *rx_pol, *code),
        RadioCommand::PowerOn => radio.power_on(),
        RadioCommand::PowerOff => radio.power_off(),
        RadioCommand::Quit => Ok(()),
    }
}
