use crate::bcd;
use crate::command::{Command, cmd};
use crate::error::{CivError, Result};
use crate::frequency::Frequency;
use crate::mode::OperatingMode;
use crate::protocol::Frame;

/// Raw GPS position data decoded from BCD nibbles (all integer fields).
///
/// Latitude/longitude stored in dd°mm.mmm format as separate integer parts.
/// Convert to decimal degrees via `RawGpsPosition::to_gps_position()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGpsPosition {
    /// Latitude degrees (0–90).
    pub lat_deg: u8,
    /// Latitude minutes integer part (0–59).
    pub lat_min: u8,
    /// Latitude minutes fractional part in thousandths (0–999).
    pub lat_min_frac: u16,
    /// true = North, false = South.
    pub lat_north: bool,
    /// Longitude degrees (0–180).
    pub lon_deg: u16,
    /// Longitude minutes integer part (0–59).
    pub lon_min: u8,
    /// Longitude minutes fractional part in thousandths (0–999).
    pub lon_min_frac: u16,
    /// true = East, false = West.
    pub lon_east: bool,
    /// Altitude in tenths of a meter (raw value before sign).
    pub alt_tenths: u32,
    /// true = negative altitude.
    pub alt_negative: bool,
    /// Course in degrees (0–359).
    pub course: u16,
    /// Speed in tenths of km/h.
    pub speed_tenths: u32,
    /// UTC year (e.g. 2026).
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

/// A typed response from the radio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// Command accepted (FB response).
    Ok,
    /// Command rejected (FA response).
    Ng,
    /// Frequency data (response to ReadFrequency or echo of SetFrequency).
    Frequency(Frequency),
    /// Operating mode and filter (response to ReadMode).
    Mode(OperatingMode),
    /// Level value (response to ReadLevel). Contains (sub_command, value).
    Level(u8, u16),
    /// Meter reading (response to ReadMeter). Contains (sub_command, value).
    Meter(u8, u16),
    /// Transceiver ID (response to ReadTransceiverId).
    TransceiverId(u8),
    /// Various function setting (response to ReadVarious). Contains (sub_command, raw_value).
    /// The value is a single raw byte, NOT BCD-decoded.
    Various(u8, u8),
    /// Duplex direction (response to ReadDuplex).
    /// 0x10=Simplex, 0x11=DUP-, 0x12=DUP+. The sub_command IS the data.
    Duplex(u8),
    /// Offset frequency (response to ReadOffset). Same 5-byte LE BCD as operating frequency.
    Offset(Frequency),
    /// Tone frequency (response to ReadTone 0x00 or 0x01).
    /// Contains (sub_command, frequency in tenths of Hz, e.g. 1413 = 141.3 Hz).
    ToneFrequency(u8, u16),
    /// DTCS code and polarity (response to ReadTone 0x02).
    /// Contains (tx_polarity, rx_polarity, code). Polarity: 0=Normal, 1=Reverse.
    DtcsCode(u8, u8, u16),
    /// GPS position data (response to ReadGpsPosition).
    GpsPosition(RawGpsPosition),
}

/// Parse a response `Frame` into a typed `Response`, using the original `Command`
/// to disambiguate commands that share the same command byte.
pub fn parse_response(frame: &Frame, command: &Command) -> Result<Response> {
    // Handle OK/NG first — these apply to any command.
    if frame.is_ok() {
        return Ok(Response::Ok);
    }
    if frame.is_ng() {
        return Ok(Response::Ng);
    }

    match command {
        Command::ReadFrequency => parse_frequency_response(frame),
        Command::SetFrequency(_) => {
            // SetFrequency gets an OK/NG (handled above), but we could
            // also receive a frequency echo.
            if frame.command == cmd::SET_FREQ || frame.command == cmd::READ_FREQ {
                parse_frequency_response(frame)
            } else {
                Err(CivError::InvalidFrame)
            }
        }
        Command::ReadMode => parse_mode_response(frame),
        Command::SetMode(_) => Ok(Response::Ok),
        Command::SelectVfoA | Command::SelectVfoB => Ok(Response::Ok),
        Command::ReadLevel(sub) => parse_level_response(frame, *sub),
        Command::SetLevel(_, _) => Ok(Response::Ok),
        Command::ReadMeter(sub) => parse_meter_response(frame, *sub),
        Command::PowerOn | Command::PowerOff => Ok(Response::Ok),
        Command::ReadTransceiverId => parse_transceiver_id_response(frame),
        Command::ReadVarious(sub) => parse_various_response(frame, *sub),
        Command::ReadDuplex => parse_duplex_response(frame),
        Command::ReadOffset => parse_offset_response(frame),
        Command::ReadTone(sub) => parse_tone_response(frame, *sub),
        Command::SetDuplex(_) => Ok(Response::Ok),
        Command::SetOffset(_) => Ok(Response::Ok),
        Command::SetVarious(_, _) => Ok(Response::Ok),
        Command::SetTone(_, _) => Ok(Response::Ok),
        Command::SetDtcs(_, _, _) => Ok(Response::Ok),
        Command::ReadGpsPosition => parse_gps_position_response(frame),
    }
}

/// Parse a frequency response frame.
///
/// The frequency is encoded as 5 BCD bytes in the frame payload.
/// In a frequency response, the payload is everything after the command byte:
/// `sub_command` (if present) + `data`.
fn parse_frequency_response(frame: &Frame) -> Result<Response> {
    let mut freq_bytes = Vec::with_capacity(5);
    if let Some(sc) = frame.sub_command {
        freq_bytes.push(sc);
    }
    freq_bytes.extend_from_slice(&frame.data);

    if freq_bytes.len() != 5 {
        return Err(CivError::InvalidFrame);
    }

    let mut arr = [0u8; 5];
    arr.copy_from_slice(&freq_bytes);
    let freq = Frequency::from_civ_bytes(arr)?;
    Ok(Response::Frequency(freq))
}

/// Parse a mode response frame.
///
/// Mode response payload: `<mode_byte> <filter_byte>`
fn parse_mode_response(frame: &Frame) -> Result<Response> {
    // The mode response has the mode byte as sub_command and filter as data[0].
    let mode_byte = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    let filter_byte = frame.data.first().copied().ok_or(CivError::InvalidFrame)?;
    let mode = OperatingMode::from_civ_bytes(mode_byte, filter_byte)?;
    Ok(Response::Mode(mode))
}

/// Parse a level response frame.
fn parse_level_response(frame: &Frame, expected_sub: u8) -> Result<Response> {
    let sub = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    if sub != expected_sub {
        return Err(CivError::InvalidFrame);
    }
    if frame.data.len() != 2 {
        return Err(CivError::InvalidFrame);
    }
    let value = bcd::decode_bcd_be(&frame.data)? as u16;
    Ok(Response::Level(sub, value))
}

/// Parse a meter response frame.
fn parse_meter_response(frame: &Frame, expected_sub: u8) -> Result<Response> {
    let sub = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    if sub != expected_sub {
        return Err(CivError::InvalidFrame);
    }
    if frame.data.len() != 2 {
        return Err(CivError::InvalidFrame);
    }
    let value = bcd::decode_bcd_be(&frame.data)? as u16;
    Ok(Response::Meter(sub, value))
}

/// Parse a transceiver ID response frame.
fn parse_transceiver_id_response(frame: &Frame) -> Result<Response> {
    let id = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    Ok(Response::TransceiverId(id))
}

/// Parse a various function response frame.
///
/// The response is a single raw byte (NOT BCD-decoded).
/// Frame format: `[cmd=0x16] [sub=0x5D] [data: 1 byte raw value]`
fn parse_various_response(frame: &Frame, expected_sub: u8) -> Result<Response> {
    let sub = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    if sub != expected_sub {
        return Err(CivError::InvalidFrame);
    }
    let value = frame.data.first().copied().ok_or(CivError::InvalidFrame)?;
    Ok(Response::Various(sub, value))
}

/// Parse a duplex direction response frame.
///
/// The sub_command byte IS the data: 0x10=Simplex, 0x11=DUP-, 0x12=DUP+.
fn parse_duplex_response(frame: &Frame) -> Result<Response> {
    let duplex = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    Ok(Response::Duplex(duplex))
}

/// Parse a duplex offset frequency response frame.
///
/// 3-byte LE BCD format (100 Hz resolution):
///   byte 0: (1 kHz)(100 Hz)
///   byte 1: (100 kHz)(10 kHz)
///   byte 2: (10 MHz)(1 MHz)
///
/// Decoded via standard LE BCD, then multiplied by 100 to get Hz.
fn parse_offset_response(frame: &Frame) -> Result<Response> {
    let mut offset_bytes = Vec::with_capacity(3);
    if let Some(sc) = frame.sub_command {
        offset_bytes.push(sc);
    }
    offset_bytes.extend_from_slice(&frame.data);

    if offset_bytes.len() != 3 {
        return Err(CivError::InvalidFrame);
    }

    // LE BCD decode gives units of 100 Hz (the smallest digit pair).
    let raw = bcd::decode_bcd_le(&offset_bytes)?;
    let hz = raw * 100;
    let freq = Frequency::from_hz(hz)?;
    Ok(Response::Offset(freq))
}

/// Parse a tone/DTCS response frame.
///
/// For sub 0x00 (Tx tone) and 0x01 (Rx tone):
///   3 bytes: `[0x00, hundreds_tens_BCD, units_tenths_BCD]`
///   Example: 141.3 Hz → `[0x00, 0x14, 0x13]` → stored as 1413 (tenths of Hz).
///
/// For sub 0x02 (DTCS):
///   3 bytes: `[polarity_nibbles, 0x0_first_digit, second_third_BCD]`
///   High nibble of byte 0 = Tx polarity (0=Normal, 1=Reverse)
///   Low nibble of byte 0 = Rx polarity
///   Example: code 023, normal → `[0x00, 0x00, 0x23]`
fn parse_tone_response(frame: &Frame, expected_sub: u8) -> Result<Response> {
    let sub = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    if sub != expected_sub {
        return Err(CivError::InvalidFrame);
    }
    if frame.data.len() != 3 {
        return Err(CivError::InvalidFrame);
    }

    match sub {
        0x00 | 0x01 => {
            // Tone frequency: [0x00, hundreds_tens, units_tenths]
            let hundreds_tens = frame.data[1];
            let units_tenths = frame.data[2];
            let ht = bcd::decode_bcd_be(&[hundreds_tens])? as u16;
            let ut = bcd::decode_bcd_be(&[units_tenths])? as u16;
            let freq_tenths = ht * 100 + ut;
            Ok(Response::ToneFrequency(sub, freq_tenths))
        }
        0x02 => {
            // DTCS code: [polarity, first_digit, second_third]
            let polarity_byte = frame.data[0];
            let tx_pol = (polarity_byte >> 4) & 0x0F;
            let rx_pol = polarity_byte & 0x0F;
            let first = bcd::decode_bcd_be(&[frame.data[1]])? as u16;
            let second_third = bcd::decode_bcd_be(&[frame.data[2]])? as u16;
            let code = first * 100 + second_third;
            Ok(Response::DtcsCode(tx_pol, rx_pol, code))
        }
        _ => Err(CivError::InvalidFrame),
    }
}

/// Extract the high nibble of a byte (the "H" digit).
fn hi(b: u8) -> u8 {
    (b >> 4) & 0x0F
}

/// Extract the low nibble of a byte (the "L" digit).
fn lo(b: u8) -> u8 {
    b & 0x0F
}

/// Parse a GPS position response frame (command 0x23, sub 0x00).
///
/// The response data contains 27 bytes of BCD-encoded position data.
/// Each byte holds two BCD digits (high nibble = H, low nibble = L).
///
/// See the user-provided byte layout documentation for full details.
fn parse_gps_position_response(frame: &Frame) -> Result<Response> {
    let sub = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    if sub != 0x00 {
        return Err(CivError::InvalidFrame);
    }
    // We expect 27 bytes of data (bytes 1–27 in the spec).
    // The sub_command byte is already consumed, so all 27 should be in frame.data.
    if frame.data.len() != 27 {
        return Err(CivError::InvalidFrame);
    }
    let d = &frame.data;

    // Bytes 1-5: Latitude (dd°mm.mmm)
    let lat_deg = hi(d[0]) * 10 + lo(d[0]);
    let lat_min = hi(d[1]) * 10 + lo(d[1]);
    let lat_min_frac = hi(d[2]) as u16 * 100 + lo(d[2]) as u16 * 10 + hi(d[3]) as u16;
    // d[3] lo = 0 fixed, d[4] hi = 0 fixed
    let lat_north = lo(d[4]) == 1;

    // Bytes 6-11: Longitude (ddd°mm.mmm)
    // d[5] hi = 0 fixed
    let lon_deg = lo(d[5]) as u16 * 100 + hi(d[6]) as u16 * 10 + lo(d[6]) as u16;
    let lon_min = hi(d[7]) * 10 + lo(d[7]);
    let lon_min_frac = hi(d[8]) as u16 * 100 + lo(d[8]) as u16 * 10 + hi(d[9]) as u16;
    // d[9] lo = 0 fixed, d[10] hi = 0 fixed
    let lon_east = lo(d[10]) == 1;

    // Bytes 12-15: Altitude (0.1m steps)
    let alt_tenths = hi(d[11]) as u32 * 100_000
        + lo(d[11]) as u32 * 10_000
        + hi(d[12]) as u32 * 1_000
        + lo(d[12]) as u32 * 100
        + hi(d[13]) as u32 * 10
        + lo(d[13]) as u32;
    // d[14] hi = 0 fixed
    let alt_negative = lo(d[14]) == 1;

    // Bytes 16-17: Course (1° steps)
    let course = hi(d[15]) as u16 * 100 + lo(d[15]) as u16 * 10 + hi(d[16]) as u16;

    // Bytes 18-20: Speed (0.1 km/h steps)
    let speed_tenths = hi(d[17]) as u32 * 100_000
        + lo(d[17]) as u32 * 10_000
        + hi(d[18]) as u32 * 1_000
        + lo(d[18]) as u32 * 100
        + hi(d[19]) as u32 * 10
        + lo(d[19]) as u32;

    // Bytes 21-27: UTC date/time (YYYYMMDDhhmmss)
    let utc_year =
        hi(d[20]) as u16 * 1000 + lo(d[20]) as u16 * 100 + hi(d[21]) as u16 * 10 + lo(d[21]) as u16;
    let utc_month = hi(d[22]) * 10 + lo(d[22]);
    let utc_day = hi(d[23]) * 10 + lo(d[23]);
    let utc_hour = hi(d[24]) * 10 + lo(d[24]);
    let utc_minute = hi(d[25]) * 10 + lo(d[25]);
    let utc_second = hi(d[26]) * 10 + lo(d[26]);

    Ok(Response::GpsPosition(RawGpsPosition {
        lat_deg,
        lat_min,
        lat_min_frac,
        lat_north,
        lon_deg,
        lon_min,
        lon_min_frac,
        lon_east,
        alt_tenths,
        alt_negative,
        course,
        speed_tenths,
        utc_year,
        utc_month,
        utc_day,
        utc_hour,
        utc_minute,
        utc_second,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::level_sub;
    use crate::command::meter_sub;
    use crate::protocol::{ADDR_CONTROLLER, ADDR_ID52, NG, OK};

    fn make_response_frame(command: u8, sub_command: Option<u8>, data: Vec<u8>) -> Frame {
        Frame {
            dst: ADDR_CONTROLLER,
            src: ADDR_ID52,
            command,
            sub_command,
            data,
        }
    }

    #[test]
    fn test_parse_ok() {
        let frame = make_response_frame(OK, None, vec![]);
        let resp = parse_response(&frame, &Command::ReadFrequency).unwrap();
        assert_eq!(resp, Response::Ok);
    }

    #[test]
    fn test_parse_ng() {
        let frame = make_response_frame(NG, None, vec![]);
        let resp = parse_response(&frame, &Command::ReadFrequency).unwrap();
        assert_eq!(resp, Response::Ng);
    }

    #[test]
    fn test_parse_frequency() {
        // 145.000.000 Hz: BCD LE = 00 00 00 45 01
        let frame = make_response_frame(cmd::READ_FREQ, Some(0x00), vec![0x00, 0x00, 0x45, 0x01]);
        let resp = parse_response(&frame, &Command::ReadFrequency).unwrap();
        assert_eq!(
            resp,
            Response::Frequency(Frequency::from_hz(145_000_000).unwrap())
        );
    }

    #[test]
    fn test_parse_mode_fm() {
        let frame = make_response_frame(cmd::READ_MODE, Some(0x05), vec![0x01]);
        let resp = parse_response(&frame, &Command::ReadMode).unwrap();
        assert_eq!(resp, Response::Mode(OperatingMode::Fm));
    }

    #[test]
    fn test_parse_mode_dv() {
        let frame = make_response_frame(cmd::READ_MODE, Some(0x17), vec![0x01]);
        let resp = parse_response(&frame, &Command::ReadMode).unwrap();
        assert_eq!(resp, Response::Mode(OperatingMode::Dv));
    }

    #[test]
    fn test_parse_level() {
        let frame = make_response_frame(cmd::LEVEL, Some(level_sub::AF_LEVEL), vec![0x01, 0x28]);
        let resp = parse_response(&frame, &Command::ReadLevel(level_sub::AF_LEVEL)).unwrap();
        assert_eq!(resp, Response::Level(level_sub::AF_LEVEL, 128));
    }

    #[test]
    fn test_parse_meter() {
        let frame = make_response_frame(cmd::METER, Some(meter_sub::S_METER), vec![0x00, 0x50]);
        let resp = parse_response(&frame, &Command::ReadMeter(meter_sub::S_METER)).unwrap();
        assert_eq!(resp, Response::Meter(meter_sub::S_METER, 50));
    }

    #[test]
    fn test_parse_transceiver_id() {
        let frame = make_response_frame(cmd::READ_ID, Some(0xB4), vec![]);
        let resp = parse_response(&frame, &Command::ReadTransceiverId).unwrap();
        assert_eq!(resp, Response::TransceiverId(0xB4));
    }

    #[test]
    fn test_parse_level_wrong_sub() {
        let frame = make_response_frame(cmd::LEVEL, Some(0x99), vec![0x01, 0x28]);
        let result = parse_response(&frame, &Command::ReadLevel(level_sub::AF_LEVEL));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_various_tone_squelch() {
        use crate::command::various_sub;
        // Tone squelch function = 0x01 (TONE Tx)
        let frame = make_response_frame(
            cmd::VARIOUS,
            Some(various_sub::TONE_SQUELCH_FUNC),
            vec![0x01],
        );
        let resp = parse_response(
            &frame,
            &Command::ReadVarious(various_sub::TONE_SQUELCH_FUNC),
        )
        .unwrap();
        assert_eq!(
            resp,
            Response::Various(various_sub::TONE_SQUELCH_FUNC, 0x01)
        );
    }

    #[test]
    fn test_parse_duplex_simplex() {
        let frame = make_response_frame(cmd::READ_DUPLEX, Some(0x10), vec![]);
        let resp = parse_response(&frame, &Command::ReadDuplex).unwrap();
        assert_eq!(resp, Response::Duplex(0x10));
    }

    #[test]
    fn test_parse_duplex_plus() {
        let frame = make_response_frame(cmd::READ_DUPLEX, Some(0x12), vec![]);
        let resp = parse_response(&frame, &Command::ReadDuplex).unwrap();
        assert_eq!(resp, Response::Duplex(0x12));
    }

    #[test]
    fn test_parse_offset() {
        // 600 kHz = 600,000 Hz. Raw in 100 Hz units = 6000.
        // 3-byte LE BCD: [0x00, 0x60, 0x00]
        // Frame: sub_command=0x00, data=[0x60, 0x00]
        let frame = make_response_frame(cmd::READ_OFFSET, Some(0x00), vec![0x60, 0x00]);
        let resp = parse_response(&frame, &Command::ReadOffset).unwrap();
        assert_eq!(resp, Response::Offset(Frequency::from_hz(600_000).unwrap()));
    }

    #[test]
    fn test_parse_offset_5mhz() {
        // 5 MHz = 5,000,000 Hz. Raw in 100 Hz units = 50000.
        // 3-byte LE BCD: [0x00, 0x00, 0x05]
        // Frame: sub_command=0x00, data=[0x00, 0x05]
        let frame = make_response_frame(cmd::READ_OFFSET, Some(0x00), vec![0x00, 0x05]);
        let resp = parse_response(&frame, &Command::ReadOffset).unwrap();
        assert_eq!(
            resp,
            Response::Offset(Frequency::from_hz(5_000_000).unwrap())
        );
    }

    #[test]
    fn test_parse_tone_frequency_tx() {
        use crate::command::tone_sub;
        // 141.3 Hz → [0x00, 0x14, 0x13]
        let frame = make_response_frame(
            cmd::TONE,
            Some(tone_sub::REPEATER_TONE),
            vec![0x00, 0x14, 0x13],
        );
        let resp = parse_response(&frame, &Command::ReadTone(tone_sub::REPEATER_TONE)).unwrap();
        assert_eq!(resp, Response::ToneFrequency(tone_sub::REPEATER_TONE, 1413));
    }

    #[test]
    fn test_parse_tone_frequency_rx() {
        use crate::command::tone_sub;
        // 88.5 Hz → [0x00, 0x08, 0x85]
        let frame =
            make_response_frame(cmd::TONE, Some(tone_sub::TSQL_TONE), vec![0x00, 0x08, 0x85]);
        let resp = parse_response(&frame, &Command::ReadTone(tone_sub::TSQL_TONE)).unwrap();
        assert_eq!(resp, Response::ToneFrequency(tone_sub::TSQL_TONE, 885));
    }

    #[test]
    fn test_parse_dtcs_code() {
        use crate::command::tone_sub;
        // Code 023, normal polarity → [0x00, 0x00, 0x23]
        let frame = make_response_frame(cmd::TONE, Some(tone_sub::DTCS), vec![0x00, 0x00, 0x23]);
        let resp = parse_response(&frame, &Command::ReadTone(tone_sub::DTCS)).unwrap();
        assert_eq!(resp, Response::DtcsCode(0, 0, 23));
    }

    #[test]
    fn test_parse_dtcs_code_reverse() {
        use crate::command::tone_sub;
        // Code 754, Tx=Reverse, Rx=Normal → [0x10, 0x07, 0x54]
        let frame = make_response_frame(cmd::TONE, Some(tone_sub::DTCS), vec![0x10, 0x07, 0x54]);
        let resp = parse_response(&frame, &Command::ReadTone(tone_sub::DTCS)).unwrap();
        assert_eq!(resp, Response::DtcsCode(1, 0, 754));
    }

    #[test]
    fn test_parse_gps_position() {
        // Example: 40°41.892'N, 074°02.536'W, Alt 10.2m, Course 125°, Speed 5.2 km/h
        // UTC: 2026-02-17 15:30:45
        //
        // Lat: 40°41.892 N
        //   Byte 1: 0x40 (4,0 → 40°)
        //   Byte 2: 0x41 (4,1 → 41')
        //   Byte 3: 0x89 (8,9 → .89_)
        //   Byte 4: 0x20 (2,0 → .xx2, 0=fixed)
        //   Byte 5: 0x01 (0=fixed, 1=North)
        //
        // Lon: 074°02.536 W
        //   Byte 6: 0x00 (0=fixed, 0=100°digit)
        //   Byte 7: 0x74 (7,4 → 74, so 074°)
        //   Byte 8: 0x02 (0,2 → 02')
        //   Byte 9: 0x53 (5,3 → .53_)
        //   Byte10: 0x60 (6,0 → .xx6, 0=fixed)
        //   Byte11: 0x00 (0=fixed, 0=West)
        //
        // Alt: 10.2m positive
        //   Byte12: 0x00 (0,0)
        //   Byte13: 0x01 (0,1)
        //   Byte14: 0x02 (0,2) → 000102 tenths = 102 → 10.2m
        //   Byte15: 0x00 (0=fixed, 0=positive)
        //
        // Course: 125°
        //   Byte16: 0x12 (1,2 → 12_)
        //   Byte17: 0x50 (5,0 → __5, 0=fixed?) → 125
        //
        // Speed: 5.2 km/h → 52 tenths
        //   Byte18: 0x00 (0,0)
        //   Byte19: 0x00 (0,0)
        //   Byte20: 0x52 (5,2) → 000052 tenths = 52
        //
        // UTC: 2026-02-17 15:30:45
        //   Byte21: 0x20 (2,0)
        //   Byte22: 0x26 (2,6) → year 2026
        //   Byte23: 0x02 (0,2) → month 02
        //   Byte24: 0x17 (1,7) → day 17
        //   Byte25: 0x15 (1,5) → hour 15
        //   Byte26: 0x30 (3,0) → minute 30
        //   Byte27: 0x45 (4,5) → second 45
        let data = vec![
            0x40, 0x41, 0x89, 0x20, 0x01, // lat
            0x00, 0x74, 0x02, 0x53, 0x60, 0x00, // lon
            0x00, 0x01, 0x02, 0x00, // alt
            0x12, 0x50, // course
            0x00, 0x00, 0x52, // speed
            0x20, 0x26, 0x02, 0x17, 0x15, 0x30, 0x45, // datetime
        ];
        let frame = make_response_frame(cmd::READ_GPS, Some(0x00), data);
        let resp = parse_response(&frame, &Command::ReadGpsPosition).unwrap();
        assert_eq!(
            resp,
            Response::GpsPosition(RawGpsPosition {
                lat_deg: 40,
                lat_min: 41,
                lat_min_frac: 892,
                lat_north: true,
                lon_deg: 74,
                lon_min: 2,
                lon_min_frac: 536,
                lon_east: false,
                alt_tenths: 102,
                alt_negative: false,
                course: 125,
                speed_tenths: 52,
                utc_year: 2026,
                utc_month: 2,
                utc_day: 17,
                utc_hour: 15,
                utc_minute: 30,
                utc_second: 45,
            })
        );
    }
}
