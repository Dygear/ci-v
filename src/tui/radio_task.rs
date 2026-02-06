use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tokio::sync::mpsc as tokio_mpsc;

use crate::radio::Radio;

use super::message::{RadioCommand, RadioEvent, RadioState, Vfo};

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

    loop {
        // Process any pending commands (non-blocking).
        match cmd_rx.try_recv() {
            Ok(RadioCommand::Quit) => {
                let _ = event_tx.send(RadioEvent::Disconnected);
                return;
            }
            Ok(cmd) => {
                if let Err(e) = execute_command(&mut radio, &cmd) {
                    let _ = event_tx.send(RadioEvent::Error(format!("{e}")));
                }
            }
            Err(std_mpsc::TryRecvError::Empty) => {}
            Err(std_mpsc::TryRecvError::Disconnected) => return,
        }

        // Poll radio state.
        let state = poll_state(&mut radio);

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
            tx_bits_per_sec,
            rx_bits_per_sec,
            ..state
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
        RadioCommand::Quit => Ok(()),
    }
}

fn poll_state(radio: &mut Radio) -> RadioState {
    RadioState {
        frequency: radio.read_frequency().ok(),
        mode: radio.read_mode().ok(),
        s_meter: radio.read_s_meter().ok(),
        af_level: radio.read_af_level().ok(),
        squelch: radio.read_squelch().ok(),
        tx_bits_per_sec: 0,
        rx_bits_per_sec: 0,
    }
}
