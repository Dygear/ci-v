use std::fmt;

use crate::frequency::Frequency;
use crate::mode::OperatingMode;

/// VFO selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vfo {
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

/// Snapshot of all radio state. `None` means not yet read or read failed.
#[derive(Debug, Clone, Default)]
pub struct RadioState {
    pub frequency: Option<Frequency>,
    pub mode: Option<OperatingMode>,
    pub s_meter: Option<u16>,
    pub af_level: Option<u16>,
    pub squelch: Option<u16>,
    pub tx_bits_per_sec: u32,
    pub rx_bits_per_sec: u32,
}
