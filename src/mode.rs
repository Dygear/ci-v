use std::fmt;

use crate::error::{CivError, Result};

/// Operating mode of the radio.
///
/// The ID-52A Plus supports FM, FM-N (narrow), AM, AM-N, and DV (D-STAR digital voice).
/// CI-V encodes the mode as a (mode_byte, filter_byte) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperatingMode {
    /// FM (wide)
    Fm,
    /// FM Narrow
    FmN,
    /// AM (wide)
    Am,
    /// AM Narrow
    AmN,
    /// D-STAR Digital Voice
    Dv,
}

/// CI-V mode byte values.
const MODE_FM: u8 = 0x05;
const MODE_AM: u8 = 0x02;
const MODE_DV: u8 = 0x17;

/// CI-V filter byte values.
const FILTER_WIDE: u8 = 0x01;
const FILTER_NARROW: u8 = 0x02;

impl OperatingMode {
    /// Decode from CI-V mode and filter bytes.
    pub fn from_civ_bytes(mode: u8, filter: u8) -> Result<Self> {
        match (mode, filter) {
            (MODE_FM, FILTER_WIDE) => Ok(Self::Fm),
            (MODE_FM, FILTER_NARROW) => Ok(Self::FmN),
            (MODE_AM, FILTER_WIDE) => Ok(Self::Am),
            (MODE_AM, FILTER_NARROW) => Ok(Self::AmN),
            (MODE_DV, _) => Ok(Self::Dv),
            _ => Err(CivError::UnknownMode(mode)),
        }
    }

    /// Encode to CI-V (mode_byte, filter_byte) pair.
    pub fn to_civ_bytes(self) -> (u8, u8) {
        match self {
            Self::Fm => (MODE_FM, FILTER_WIDE),
            Self::FmN => (MODE_FM, FILTER_NARROW),
            Self::Am => (MODE_AM, FILTER_WIDE),
            Self::AmN => (MODE_AM, FILTER_NARROW),
            Self::Dv => (MODE_DV, FILTER_WIDE),
        }
    }

    /// Toggle between wide and narrow variants. DV has no narrow variant and stays unchanged.
    pub fn toggle_width(self) -> Self {
        match self {
            Self::Fm => Self::FmN,
            Self::FmN => Self::Fm,
            Self::Am => Self::AmN,
            Self::AmN => Self::Am,
            Self::Dv => Self::Dv,
        }
    }

    /// Returns true if this is a narrow (12.5 kHz) mode.
    pub fn is_narrow(self) -> bool {
        matches!(self, Self::FmN | Self::AmN)
    }
}

impl fmt::Display for OperatingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fm => write!(f, "FM"),
            Self::FmN => write!(f, "FM-N"),
            Self::Am => write!(f, "AM"),
            Self::AmN => write!(f, "AM-N"),
            Self::Dv => write!(f, "DV"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fm_roundtrip() {
        let mode = OperatingMode::Fm;
        let (m, f) = mode.to_civ_bytes();
        assert_eq!(OperatingMode::from_civ_bytes(m, f).unwrap(), mode);
    }

    #[test]
    fn test_fm_narrow_roundtrip() {
        let mode = OperatingMode::FmN;
        let (m, f) = mode.to_civ_bytes();
        assert_eq!(OperatingMode::from_civ_bytes(m, f).unwrap(), mode);
    }

    #[test]
    fn test_am_roundtrip() {
        let mode = OperatingMode::Am;
        let (m, f) = mode.to_civ_bytes();
        assert_eq!(OperatingMode::from_civ_bytes(m, f).unwrap(), mode);
    }

    #[test]
    fn test_am_narrow_roundtrip() {
        let mode = OperatingMode::AmN;
        let (m, f) = mode.to_civ_bytes();
        assert_eq!(OperatingMode::from_civ_bytes(m, f).unwrap(), mode);
    }

    #[test]
    fn test_dv_roundtrip() {
        let mode = OperatingMode::Dv;
        let (m, f) = mode.to_civ_bytes();
        assert_eq!(OperatingMode::from_civ_bytes(m, f).unwrap(), mode);
    }

    #[test]
    fn test_unknown_mode() {
        assert!(OperatingMode::from_civ_bytes(0xFF, 0x01).is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", OperatingMode::Fm), "FM");
        assert_eq!(format!("{}", OperatingMode::FmN), "FM-N");
        assert_eq!(format!("{}", OperatingMode::Am), "AM");
        assert_eq!(format!("{}", OperatingMode::AmN), "AM-N");
        assert_eq!(format!("{}", OperatingMode::Dv), "DV");
    }

    #[test]
    fn test_civ_byte_values() {
        assert_eq!(OperatingMode::Fm.to_civ_bytes(), (0x05, 0x01));
        assert_eq!(OperatingMode::FmN.to_civ_bytes(), (0x05, 0x02));
        assert_eq!(OperatingMode::Am.to_civ_bytes(), (0x02, 0x01));
        assert_eq!(OperatingMode::AmN.to_civ_bytes(), (0x02, 0x02));
        assert_eq!(OperatingMode::Dv.to_civ_bytes(), (0x17, 0x01));
    }
}
