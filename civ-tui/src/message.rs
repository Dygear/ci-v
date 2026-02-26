use civ_protocol::Frequency;
use civ_protocol::OperatingMode;

// Domain types re-exported from the protocol library.
pub use civ_protocol::{GpsPosition, RadioState, Vfo, VfoState};

/// Commands sent from the TUI to the radio task.
#[derive(Debug)]
pub enum RadioCommand {
    SetFrequency(Frequency),
    SetMode(OperatingMode),
    SetAfLevel(u16),
    SetSquelch(u16),
    SelectVfo(Vfo),
    /// Set RF power level (raw 0–255).
    SetRfPower(u16),
    /// Set duplex direction (0x10=Simplex, 0x11=DUP-, 0x12=DUP+).
    SetDuplex(u8),
    /// Set duplex offset frequency in Hz.
    SetOffset(u64),
    /// Set the tone/squelch function mode (0x00–0x09).
    SetToneMode(u8),
    /// Set Tx tone frequency (tenths of Hz, e.g. 1318 = 131.8 Hz).
    SetTxTone(u16),
    /// Set Rx tone frequency (tenths of Hz).
    SetRxTone(u16),
    /// Set DTCS code and polarity (tx_pol, rx_pol, code).
    SetDtcsCode(u8, u8, u16),
    /// Power on the radio (with wake-up preamble).
    PowerOn,
    /// Power off the radio.
    PowerOff,
    Quit,
}

/// Events sent from the radio task to the TUI.
#[derive(Debug)]
pub enum RadioEvent {
    StateUpdate(RadioState),
    Error(String),
    Info(String),
    Connected,
    Disconnected,
}
