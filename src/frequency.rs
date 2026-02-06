use std::fmt;

use crate::bcd;
use crate::error::{CivError, Result};

/// A radio frequency stored as Hz.
///
/// CI-V encodes frequencies as 5 BCD bytes in little-endian order,
/// giving 10 decimal digits with 1 Hz resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Frequency(u64);

impl Frequency {
    /// Create a `Frequency` from a value in Hz.
    pub fn from_hz(hz: u64) -> Result<Self> {
        if hz > 9_999_999_999 {
            return Err(CivError::FrequencyOutOfRange(hz));
        }
        Ok(Self(hz))
    }

    /// Create a `Frequency` from a value in kHz.
    pub fn from_khz(khz: f64) -> Result<Self> {
        Self::from_hz((khz * 1_000.0) as u64)
    }

    /// Create a `Frequency` from a value in MHz.
    pub fn from_mhz(mhz: f64) -> Result<Self> {
        Self::from_hz((mhz * 1_000_000.0) as u64)
    }

    /// Return the frequency in Hz.
    pub fn hz(self) -> u64 {
        self.0
    }

    /// Return the frequency in kHz.
    pub fn khz(self) -> f64 {
        self.0 as f64 / 1_000.0
    }

    /// Return the frequency in MHz.
    pub fn mhz(self) -> f64 {
        self.0 as f64 / 1_000_000.0
    }

    /// Decode a frequency from 5 CI-V BCD bytes (little-endian, 1 Hz resolution).
    pub fn from_civ_bytes(bytes: [u8; 5]) -> Result<Self> {
        let hz = bcd::decode_bcd_le(&bytes)?;
        Self::from_hz(hz)
    }

    /// Encode the frequency to 5 CI-V BCD bytes (little-endian, 1 Hz resolution).
    pub fn to_civ_bytes(self) -> Result<[u8; 5]> {
        let vec = bcd::encode_bcd_le(self.0, 5)?;
        let mut arr = [0u8; 5];
        arr.copy_from_slice(&vec);
        Ok(arr)
    }
}

impl fmt::Display for Frequency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mhz = self.0 / 1_000_000;
        let khz = (self.0 % 1_000_000) / 1_000;
        let hz = self.0 % 1_000;
        write!(f, "{mhz}.{khz:03}.{hz:03} MHz")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_hz() {
        let freq = Frequency::from_hz(145_000_000).unwrap();
        assert_eq!(freq.hz(), 145_000_000);
    }

    #[test]
    fn test_from_khz() {
        let freq = Frequency::from_khz(145_000.0).unwrap();
        assert_eq!(freq.hz(), 145_000_000);
    }

    #[test]
    fn test_from_mhz() {
        let freq = Frequency::from_mhz(145.0).unwrap();
        assert_eq!(freq.hz(), 145_000_000);
    }

    #[test]
    fn test_out_of_range() {
        assert!(Frequency::from_hz(10_000_000_000).is_err());
    }

    #[test]
    fn test_civ_roundtrip_vhf() {
        let freq = Frequency::from_mhz(145.5).unwrap();
        let bytes = freq.to_civ_bytes().unwrap();
        let decoded = Frequency::from_civ_bytes(bytes).unwrap();
        assert_eq!(freq, decoded);
    }

    #[test]
    fn test_civ_roundtrip_uhf() {
        let freq = Frequency::from_hz(430_250_000).unwrap();
        let bytes = freq.to_civ_bytes().unwrap();
        assert_eq!(bytes, [0x00, 0x00, 0x25, 0x30, 0x04]);
        let decoded = Frequency::from_civ_bytes(bytes).unwrap();
        assert_eq!(freq, decoded);
    }

    #[test]
    fn test_display() {
        let freq = Frequency::from_hz(145_500_000).unwrap();
        assert_eq!(format!("{freq}"), "145.500.000 MHz");
    }

    #[test]
    fn test_display_with_hz() {
        let freq = Frequency::from_hz(145_012_500).unwrap();
        assert_eq!(format!("{freq}"), "145.012.500 MHz");
    }

    #[test]
    fn test_accessors() {
        let freq = Frequency::from_hz(145_500_000).unwrap();
        assert!((freq.khz() - 145_500.0).abs() < f64::EPSILON);
        assert!((freq.mhz() - 145.5).abs() < f64::EPSILON);
    }
}
