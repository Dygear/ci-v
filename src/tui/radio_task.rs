use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tokio::sync::mpsc as tokio_mpsc;

use crate::radio::Radio;

use super::message::{RadioCommand, RadioEvent, RadioState, Vfo, VfoState};

/// Bits per byte on the wire with 8N1 framing (1 start + 8 data + 1 stop).
const BITS_PER_BYTE: u64 = 10;

/// Run the radio polling loop on a blocking thread.
///
/// Reads radio state every ~200ms and sends updates via `event_tx`.
/// Executes commands received on `cmd_rx` immediately.
pub fn radio_loop(
    mut radio: Radio,
    cmd_rx: std_mpsc::Receiver<RadioCommand>,
    event_tx: tokio_mpsc::UnboundedSender<RadioEvent>,
) {
    let _ = event_tx.send(RadioEvent::Connected);

    let mut last_rate_time = Instant::now();
    let mut last_tx_bytes: u64 = 0;
    let mut last_rx_bytes: u64 = 0;
    let mut tx_bits_per_sec: u32 = 0;
    let mut rx_bits_per_sec: u32 = 0;

    // Per-VFO caches — we only poll the active VFO, so cache the other.
    let mut active_vfo = Vfo::A;
    let mut cached_vfo_a = VfoState::default();
    let mut cached_vfo_b = VfoState::default();

    loop {
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

        // Poll radio state for the active VFO.
        let (vfo_state, s_meter, af_level, squelch) = poll_state(&mut radio);

        // Update the active VFO's cache.
        match active_vfo {
            Vfo::A => cached_vfo_a = vfo_state,
            Vfo::B => cached_vfo_b = vfo_state,
        }

        // Compute bits-per-second rates from byte counters.
        let elapsed = last_rate_time.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            let tx_delta = radio.tx_bytes() - last_tx_bytes;
            let rx_delta = radio.rx_bytes() - last_rx_bytes;
            tx_bits_per_sec = (tx_delta as f64 * BITS_PER_BYTE as f64 / elapsed).round() as u32;
            rx_bits_per_sec = (rx_delta as f64 * BITS_PER_BYTE as f64 / elapsed).round() as u32;
            last_tx_bytes = radio.tx_bytes();
            last_rx_bytes = radio.rx_bytes();
            last_rate_time = Instant::now();
        }

        let state = RadioState {
            vfo_a: cached_vfo_a.clone(),
            vfo_b: cached_vfo_b.clone(),
            s_meter,
            af_level,
            squelch,
            tx_bits_per_sec,
            rx_bits_per_sec,
        };

        if event_tx.send(RadioEvent::StateUpdate(state)).is_err() {
            return;
        }

        thread::sleep(Duration::from_millis(200));
    }
}

fn execute_command(radio: &mut Radio, cmd: &RadioCommand) -> crate::Result<()> {
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
        RadioCommand::SetToneMode(mode) => radio.set_tone_mode(*mode),
        RadioCommand::SetTxTone(freq) => radio.set_tx_tone(*freq),
        RadioCommand::SetRxTone(freq) => radio.set_rx_tone(*freq),
        RadioCommand::SetDtcsCode(tx_pol, rx_pol, code) => radio.set_dtcs(*tx_pol, *rx_pol, *code),
        RadioCommand::Quit => Ok(()),
    }
}

fn poll_state(radio: &mut Radio) -> (VfoState, Option<u16>, Option<u16>, Option<u16>) {
    let frequency = radio.read_frequency().ok();
    let mode = radio.read_mode().ok();
    let rf_power = radio.read_rf_power().ok();
    let tone_mode = radio.read_tone_mode().ok();
    let duplex = radio.read_duplex().ok();
    let offset = radio.read_offset().ok();
    let tx_tone_freq = radio.read_tx_tone().ok();
    let rx_tone_freq = radio.read_rx_tone().ok();
    let dtcs = radio.read_dtcs().ok();

    let s_meter = radio.read_s_meter().ok();
    let af_level = radio.read_af_level().ok();
    let squelch = radio.read_squelch().ok();

    let vfo_state = VfoState {
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
    };

    (vfo_state, s_meter, af_level, squelch)
}
