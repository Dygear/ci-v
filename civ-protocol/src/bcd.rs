use crate::error::{CivError, Result};

/// Decode a single BCD-encoded byte into its decimal value (0–99).
pub fn decode_bcd_byte(byte: u8) -> Result<u8> {
    let high = byte >> 4;
    let low = byte & 0x0F;
    if high > 9 || low > 9 {
        return Err(CivError::InvalidBcd(byte));
    }
    Ok(high * 10 + low)
}

/// Encode a decimal value (0–99) into a single BCD byte.
pub fn encode_bcd_byte(value: u8) -> Result<u8> {
    if value > 99 {
        return Err(CivError::InvalidBcd(value));
    }
    Ok((value / 10) << 4 | (value % 10))
}

/// Decode a little-endian BCD byte slice into a `u64`.
///
/// Each byte holds two decimal digits. The least-significant digits come first.
/// For example, `[0x00, 0x50, 0x14]` decodes to `145000` (reading pairs right-to-left: 14 50 00).
pub fn decode_bcd_le(bytes: &[u8]) -> Result<u64> {
    let mut result: u64 = 0;
    for &byte in bytes.iter().rev() {
        let decoded = decode_bcd_byte(byte)? as u64;
        result = result * 100 + decoded;
    }
    Ok(result)
}

/// Encode a `u64` value into little-endian BCD, filling exactly `len` bytes.
pub fn encode_bcd_le(value: u64, len: usize) -> Result<Vec<u8>> {
    let mut result = Vec::with_capacity(len);
    let mut remaining = value;
    for _ in 0..len {
        let pair = (remaining % 100) as u8;
        result.push(encode_bcd_byte(pair)?);
        remaining /= 100;
    }
    Ok(result)
}

/// Decode a big-endian BCD byte slice into a `u64`.
///
/// Each byte holds two decimal digits. The most-significant digits come first.
/// For example, `[0x01, 0x28]` decodes to `128`.
pub fn decode_bcd_be(bytes: &[u8]) -> Result<u64> {
    let mut result: u64 = 0;
    for &byte in bytes {
        let decoded = decode_bcd_byte(byte)? as u64;
        result = result * 100 + decoded;
    }
    Ok(result)
}

/// Encode a `u64` value into big-endian BCD, filling exactly `len` bytes.
pub fn encode_bcd_be(value: u64, len: usize) -> Result<Vec<u8>> {
    let mut result = vec![0u8; len];
    let mut remaining = value;
    for i in (0..len).rev() {
        let pair = (remaining % 100) as u8;
        result[i] = encode_bcd_byte(pair)?;
        remaining /= 100;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_bcd_byte() {
        assert_eq!(decode_bcd_byte(0x00).unwrap(), 0);
        assert_eq!(decode_bcd_byte(0x09).unwrap(), 9);
        assert_eq!(decode_bcd_byte(0x10).unwrap(), 10);
        assert_eq!(decode_bcd_byte(0x99).unwrap(), 99);
        assert_eq!(decode_bcd_byte(0x45).unwrap(), 45);
    }

    #[test]
    fn test_decode_bcd_byte_invalid() {
        assert!(decode_bcd_byte(0xAF).is_err());
        assert!(decode_bcd_byte(0x0A).is_err());
        assert!(decode_bcd_byte(0xF0).is_err());
    }

    #[test]
    fn test_encode_bcd_byte() {
        assert_eq!(encode_bcd_byte(0).unwrap(), 0x00);
        assert_eq!(encode_bcd_byte(9).unwrap(), 0x09);
        assert_eq!(encode_bcd_byte(10).unwrap(), 0x10);
        assert_eq!(encode_bcd_byte(99).unwrap(), 0x99);
        assert_eq!(encode_bcd_byte(45).unwrap(), 0x45);
    }

    #[test]
    fn test_encode_bcd_byte_invalid() {
        assert!(encode_bcd_byte(100).is_err());
    }

    #[test]
    fn test_decode_bcd_le_frequency() {
        // 145.000.000 Hz = 0x00 0x00 0x00 0x45 0x01 (LE)
        // 10-digit BCD: 0145000000, LE pairs: 00 00 00 45 01
        let bytes = [0x00, 0x00, 0x00, 0x45, 0x01];
        assert_eq!(decode_bcd_le(&bytes).unwrap(), 145_000_000);
    }

    #[test]
    fn test_decode_bcd_le_uhf() {
        // 430.250.000 Hz = 0x00 0x00 0x25 0x30 0x04 (LE)
        let bytes = [0x00, 0x00, 0x25, 0x30, 0x04];
        assert_eq!(decode_bcd_le(&bytes).unwrap(), 430_250_000);
    }

    #[test]
    fn test_encode_bcd_le_frequency() {
        let bytes = encode_bcd_le(145_000_000, 5).unwrap();
        assert_eq!(bytes, vec![0x00, 0x00, 0x00, 0x45, 0x01]);
    }

    #[test]
    fn test_encode_bcd_le_uhf() {
        let bytes = encode_bcd_le(430_250_000, 5).unwrap();
        assert_eq!(bytes, vec![0x00, 0x00, 0x25, 0x30, 0x04]);
    }

    #[test]
    fn test_decode_bcd_be_level() {
        // Level 128 = 0x01 0x28 (BE)
        let bytes = [0x01, 0x28];
        assert_eq!(decode_bcd_be(&bytes).unwrap(), 128);
    }

    #[test]
    fn test_encode_bcd_be_level() {
        let bytes = encode_bcd_be(128, 2).unwrap();
        assert_eq!(bytes, vec![0x01, 0x28]);
    }

    #[test]
    fn test_decode_bcd_be_max_level() {
        let bytes = [0x02, 0x55];
        assert_eq!(decode_bcd_be(&bytes).unwrap(), 255);
    }

    #[test]
    fn test_encode_bcd_be_max_level() {
        let bytes = encode_bcd_be(255, 2).unwrap();
        assert_eq!(bytes, vec![0x02, 0x55]);
    }

    #[test]
    fn test_roundtrip_le() {
        for value in [0u64, 1, 100, 12345, 999_999_999_9] {
            let encoded = encode_bcd_le(value, 5).unwrap();
            let decoded = decode_bcd_le(&encoded).unwrap();
            assert_eq!(decoded, value, "roundtrip failed for {value}");
        }
    }

    #[test]
    fn test_roundtrip_be() {
        for value in [0u64, 1, 50, 128, 255] {
            let encoded = encode_bcd_be(value, 2).unwrap();
            let decoded = decode_bcd_be(&encoded).unwrap();
            assert_eq!(decoded, value, "roundtrip failed for {value}");
        }
    }
}
