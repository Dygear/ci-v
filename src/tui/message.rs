use std::fmt;

use crate::frequency::Frequency;
use crate::mode::OperatingMode;

/// VFO selection.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Vfo {
    #[default]
    A,
    B,
}

impl Vfo {
    /// Toggle between A and B.
    pub fn toggle(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

impl fmt::Display for Vfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::A => write!(f, "A"),
            Self::B => write!(f, "B"),
        }
    }
}

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
    Quit,
}

/// Events sent from the radio task to the TUI.
#[derive(Debug)]
pub enum RadioEvent {
    StateUpdate(RadioState),
    Error(String),
    Connected,
    Disconnected,
}

/// Per-VFO state (frequency, mode, and tone/duplex settings).
#[derive(Debug, Clone, Default)]
pub struct VfoState {
    pub frequency: Option<Frequency>,
    pub mode: Option<OperatingMode>,
    pub rf_power: Option<u16>,
    /// Combined tone/squelch function (0x00–0x09 from 0x16/0x5D).
    pub tone_mode: Option<u8>,
    /// Tx tone frequency in tenths of Hz (e.g. 1413 = 141.3 Hz).
    pub tx_tone_freq: Option<u16>,
    /// Rx tone frequency in tenths of Hz.
    pub rx_tone_freq: Option<u16>,
    /// DTCS code (e.g. 23, 754).
    pub dtcs_code: Option<u16>,
    /// DTCS Tx polarity (0=Normal, 1=Reverse).
    pub dtcs_tx_pol: Option<u8>,
    /// DTCS Rx polarity (0=Normal, 1=Reverse).
    pub dtcs_rx_pol: Option<u8>,
    /// Duplex direction (0x10=Simplex, 0x11=DUP-, 0x12=DUP+).
    pub duplex: Option<u8>,
    /// Offset frequency.
    pub offset: Option<Frequency>,
}

/// GPS position data from the radio's built-in receiver.
#[derive(Debug, Clone, Default)]
pub struct GpsPosition {
    /// Latitude in decimal degrees (negative = South).
    pub latitude: f64,
    /// Longitude in decimal degrees (negative = West).
    pub longitude: f64,
    /// Altitude in meters (negative = below sea level).
    pub altitude: f64,
    /// Course/heading in degrees (0–359).
    pub course: u16,
    /// Speed in km/h.
    pub speed: f64,
    /// UTC year.
    pub utc_year: u16,
    /// UTC month (1–12).
    pub utc_month: u8,
    /// UTC day (1–31).
    pub utc_day: u8,
    /// UTC hour (0–23).
    pub utc_hour: u8,
    /// UTC minute (0–59).
    pub utc_minute: u8,
    /// UTC second (0–59).
    pub utc_second: u8,
}

/// Snapshot of all radio state. `None` means not yet read or read failed.
#[derive(Debug, Clone, Default)]
pub struct RadioState {
    pub vfo_a: VfoState,
    pub vfo_b: VfoState,
    pub s_meter: Option<u16>,
    pub af_level: Option<u16>,
    pub squelch: Option<u16>,
    pub gps_position: Option<GpsPosition>,
    pub tx_bits_per_sec: u32,
    pub rx_bits_per_sec: u32,
}
