use crate::bcd;
use crate::error::Result;
use crate::frequency::Frequency;
use crate::mode::OperatingMode;
use crate::protocol::Frame;

/// CI-V command bytes.
pub mod cmd {
    /// Read the currently displayed frequency.
    pub const READ_FREQ: u8 = 0x03;
    /// Set the operating frequency.
    pub const SET_FREQ: u8 = 0x05;
    /// Read the current operating mode and filter.
    pub const READ_MODE: u8 = 0x04;
    /// Set the operating mode and filter.
    pub const SET_MODE: u8 = 0x06;
    /// Select VFO/memory mode.
    pub const VFO_MODE: u8 = 0x07;
    /// Read/write level settings (AF gain, squelch, RF gain, etc.).
    pub const LEVEL: u8 = 0x14;
    /// Read S-meter / power meter / SWR meter.
    pub const METER: u8 = 0x15;
    /// Read/write various function settings (tone squelch, etc.).
    pub const VARIOUS: u8 = 0x16;
    /// Send/read tone/DTCS frequency and code settings.
    pub const TONE: u8 = 0x1B;
    /// Read duplex offset frequency.
    pub const READ_OFFSET: u8 = 0x0C;
    /// Set duplex offset frequency.
    pub const SET_OFFSET: u8 = 0x0D;
    /// Read/set duplex direction.
    pub const READ_DUPLEX: u8 = 0x0F;
    /// Power on/off control.
    pub const POWER: u8 = 0x18;
    /// Read transceiver ID.
    pub const READ_ID: u8 = 0x19;
    /// Read GPS position data (My Position).
    pub const READ_GPS: u8 = 0x23;
}

/// Sub-commands for the LEVEL (0x14) command.
pub mod level_sub {
    /// AF output level (volume).
    pub const AF_LEVEL: u8 = 0x01;
    /// RF gain level.
    pub const RF_GAIN: u8 = 0x02;
    /// Squelch level.
    pub const SQUELCH: u8 = 0x03;
    /// RF power level.
    pub const RF_POWER: u8 = 0x0A;
}

/// Sub-commands for the VARIOUS (0x16) command.
pub mod various_sub {
    /// Combined tone/squelch function (returns 0x00–0x09).
    pub const TONE_SQUELCH_FUNC: u8 = 0x5D;
}

/// Sub-commands for the TONE (0x1B) command.
pub mod tone_sub {
    /// Repeater tone (Tx) frequency — 3 bytes BCD.
    pub const REPEATER_TONE: u8 = 0x00;
    /// TSQL tone (Rx) frequency — 3 bytes BCD.
    pub const TSQL_TONE: u8 = 0x01;
    /// DTCS code and polarity — 3 bytes.
    pub const DTCS: u8 = 0x02;
}

/// Sub-commands for the METER (0x15) command.
pub mod meter_sub {
    /// S-meter reading.
    pub const S_METER: u8 = 0x02;
    /// Power meter reading.
    pub const POWER_METER: u8 = 0x11;
}

/// Sub-commands for the VFO_MODE (0x07) command.
///
/// The ID-52A Plus uses 0xD0/0xD1 for A/B band selection,
/// not the 0x00/0x01 used by HF rigs.
pub mod vfo_sub {
    /// Select A band (single watch) / set MAIN band as A (dualwatch).
    pub const VFO_A: u8 = 0xD0;
    /// Select B band (single watch) / set MAIN band as B (dualwatch).
    pub const VFO_B: u8 = 0xD1;
}

/// Sub-commands for the POWER (0x18) command.
pub mod power_sub {
    /// Power off.
    pub const OFF: u8 = 0x00;
    /// Power on.
    pub const ON: u8 = 0x01;
}

/// A CI-V command to send to the radio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Read the currently displayed frequency.
    ReadFrequency,
    /// Set the operating frequency.
    SetFrequency(Frequency),
    /// Read the current operating mode and filter.
    ReadMode,
    /// Set the operating mode and filter.
    SetMode(OperatingMode),
    /// Select VFO/Band A.
    SelectVfoA,
    /// Select VFO/Band B.
    SelectVfoB,
    /// Read a level setting. The `u8` is the level sub-command.
    ReadLevel(u8),
    /// Set a level setting. The `u8` is the level sub-command, `u16` is the value (0–255).
    SetLevel(u8, u16),
    /// Read a meter value. The `u8` is the meter sub-command.
    ReadMeter(u8),
    /// Power on the radio.
    PowerOn,
    /// Power off the radio.
    PowerOff,
    /// Read the transceiver ID.
    ReadTransceiverId,
    /// Read a various function setting. The `u8` is the sub-command (e.g. 0x5D).
    ReadVarious(u8),
    /// Read duplex direction (0x10=Simplex, 0x11=DUP-, 0x12=DUP+).
    ReadDuplex,
    /// Read duplex offset frequency (5-byte LE BCD, same as operating frequency).
    ReadOffset,
    /// Read a tone/DTCS setting. The `u8` is the sub-command (0x00=Tx tone, 0x01=Rx tone, 0x02=DTCS).
    ReadTone(u8),
    /// Set duplex direction (0x10=Simplex, 0x11=DUP-, 0x12=DUP+).
    SetDuplex(u8),
    /// Set duplex offset frequency (3-byte LE BCD, 100 Hz resolution).
    SetOffset(u64),
    /// Write a various function setting. (sub_command, value).
    SetVarious(u8, u8),
    /// Write a tone frequency. (sub_command 0x00=Tx or 0x01=Rx, freq in tenths of Hz).
    SetTone(u8, u16),
    /// Write DTCS code and polarity. (tx_pol, rx_pol, code).
    SetDtcs(u8, u8, u16),
    /// Read GPS position data (command 0x23, sub 0x00).
    ReadGpsPosition,
}

impl Command {
    /// Convert this command into a CI-V `Frame` ready for transmission.
    pub fn to_frame(&self) -> Result<Frame> {
        let frame = match self {
            Command::ReadFrequency => Frame::new(cmd::READ_FREQ, None, vec![]),
            Command::SetFrequency(freq) => {
                let bytes = freq.to_civ_bytes()?;
                Frame::new(cmd::SET_FREQ, None, bytes.to_vec())
            }
            Command::ReadMode => Frame::new(cmd::READ_MODE, None, vec![]),
            Command::SetMode(mode) => {
                let (m, f) = mode.to_civ_bytes();
                Frame::new(cmd::SET_MODE, None, vec![m, f])
            }
            Command::SelectVfoA => Frame::new(cmd::VFO_MODE, Some(vfo_sub::VFO_A), vec![]),
            Command::SelectVfoB => Frame::new(cmd::VFO_MODE, Some(vfo_sub::VFO_B), vec![]),
            Command::ReadLevel(sub) => Frame::new(cmd::LEVEL, Some(*sub), vec![]),
            Command::SetLevel(sub, value) => {
                let data = bcd::encode_bcd_be(*value as u64, 2)?;
                Frame::new(cmd::LEVEL, Some(*sub), data)
            }
            Command::ReadMeter(sub) => Frame::new(cmd::METER, Some(*sub), vec![]),
            Command::PowerOn => Frame::new(cmd::POWER, Some(power_sub::ON), vec![]),
            Command::PowerOff => Frame::new(cmd::POWER, Some(power_sub::OFF), vec![]),
            Command::ReadTransceiverId => Frame::new(cmd::READ_ID, Some(0x00), vec![]),
            Command::ReadVarious(sub) => Frame::new(cmd::VARIOUS, Some(*sub), vec![]),
            Command::ReadDuplex => Frame::new(cmd::READ_DUPLEX, None, vec![]),
            Command::ReadOffset => Frame::new(cmd::READ_OFFSET, None, vec![]),
            Command::ReadTone(sub) => Frame::new(cmd::TONE, Some(*sub), vec![]),
            Command::SetDuplex(dir) => Frame::new(cmd::READ_DUPLEX, Some(*dir), vec![]),
            Command::SetOffset(hz) => {
                // Encode as 3-byte LE BCD with 100 Hz resolution.
                let raw = hz / 100;
                let data = bcd::encode_bcd_le(raw, 3)?;
                Frame::new(cmd::SET_OFFSET, None, data)
            }
            Command::SetVarious(sub, value) => Frame::new(cmd::VARIOUS, Some(*sub), vec![*value]),
            Command::SetTone(sub, freq_tenths) => {
                // Encode tone frequency as 3 bytes: [0x00, hundreds_tens_BCD, units_tenths_BCD]
                let ht = (*freq_tenths / 100) as u8;
                let ut = (*freq_tenths % 100) as u8;
                let ht_bcd = ((ht / 10) << 4) | (ht % 10);
                let ut_bcd = ((ut / 10) << 4) | (ut % 10);
                Frame::new(cmd::TONE, Some(*sub), vec![0x00, ht_bcd, ut_bcd])
            }
            Command::ReadGpsPosition => Frame::new(cmd::READ_GPS, Some(0x00), vec![]),
            Command::SetDtcs(tx_pol, rx_pol, code) => {
                // Encode DTCS as 3 bytes: [polarity_nibbles, first_digit_BCD, second_third_BCD]
                let polarity = (tx_pol << 4) | (rx_pol & 0x0F);
                let first = (*code / 100) as u8;
                let second_third = (*code % 100) as u8;
                let first_bcd = ((first / 10) << 4) | (first % 10);
                let st_bcd = ((second_third / 10) << 4) | (second_third % 10);
                Frame::new(
                    cmd::TONE,
                    Some(tone_sub::DTCS),
                    vec![polarity, first_bcd, st_bcd],
                )
            }
        };
        Ok(frame)
    }

    /// Return the command byte for this command.
    pub fn command_byte(&self) -> u8 {
        match self {
            Command::ReadFrequency => cmd::READ_FREQ,
            Command::SetFrequency(_) => cmd::SET_FREQ,
            Command::ReadMode => cmd::READ_MODE,
            Command::SetMode(_) => cmd::SET_MODE,
            Command::SelectVfoA | Command::SelectVfoB => cmd::VFO_MODE,
            Command::ReadLevel(_) | Command::SetLevel(_, _) => cmd::LEVEL,
            Command::ReadMeter(_) => cmd::METER,
            Command::PowerOn | Command::PowerOff => cmd::POWER,
            Command::ReadTransceiverId => cmd::READ_ID,
            Command::ReadVarious(_) | Command::SetVarious(_, _) => cmd::VARIOUS,
            Command::ReadDuplex | Command::SetDuplex(_) => cmd::READ_DUPLEX,
            Command::ReadOffset => cmd::READ_OFFSET,
            Command::SetOffset(_) => cmd::SET_OFFSET,
            Command::ReadTone(_) | Command::SetTone(_, _) | Command::SetDtcs(_, _, _) => cmd::TONE,
            Command::ReadGpsPosition => cmd::READ_GPS,
        }
    }

    /// Return the sub-command byte, if any.
    pub fn sub_command_byte(&self) -> Option<u8> {
        match self {
            Command::ReadFrequency
            | Command::SetFrequency(_)
            | Command::ReadMode
            | Command::SetMode(_) => None,
            Command::SelectVfoA => Some(vfo_sub::VFO_A),
            Command::SelectVfoB => Some(vfo_sub::VFO_B),
            Command::ReadLevel(sub) | Command::SetLevel(sub, _) => Some(*sub),
            Command::ReadMeter(sub) => Some(*sub),
            Command::PowerOn => Some(power_sub::ON),
            Command::PowerOff => Some(power_sub::OFF),
            Command::ReadTransceiverId => Some(0x00),
            Command::ReadVarious(sub) | Command::SetVarious(sub, _) => Some(*sub),
            Command::ReadDuplex => None,
            Command::SetDuplex(dir) => Some(*dir),
            Command::ReadOffset | Command::SetOffset(_) => None,
            Command::ReadTone(sub) | Command::SetTone(sub, _) => Some(*sub),
            Command::SetDtcs(_, _, _) => Some(tone_sub::DTCS),
            Command::ReadGpsPosition => Some(0x00),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_frequency_frame() {
        let frame = Command::ReadFrequency.to_frame().unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![0xFE, 0xFE, 0xB4, 0xE0, 0x03, 0xFD]);
    }

    #[test]
    fn test_set_frequency_frame() {
        let freq = Frequency::from_hz(145_000_000).unwrap();
        let frame = Command::SetFrequency(freq).to_frame().unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(
            bytes,
            vec![
                0xFE, 0xFE, 0xB4, 0xE0, 0x05, 0x00, 0x00, 0x00, 0x45, 0x01, 0xFD
            ]
        );
    }

    #[test]
    fn test_read_mode_frame() {
        let frame = Command::ReadMode.to_frame().unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![0xFE, 0xFE, 0xB4, 0xE0, 0x04, 0xFD]);
    }

    #[test]
    fn test_set_mode_fm_frame() {
        let frame = Command::SetMode(OperatingMode::Fm).to_frame().unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![0xFE, 0xFE, 0xB4, 0xE0, 0x06, 0x05, 0x01, 0xFD]);
    }

    #[test]
    fn test_vfo_select_a() {
        let frame = Command::SelectVfoA.to_frame().unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![0xFE, 0xFE, 0xB4, 0xE0, 0x07, 0xD0, 0xFD]);
    }

    #[test]
    fn test_read_af_level() {
        let frame = Command::ReadLevel(level_sub::AF_LEVEL).to_frame().unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![0xFE, 0xFE, 0xB4, 0xE0, 0x14, 0x01, 0xFD]);
    }

    #[test]
    fn test_set_af_level() {
        let frame = Command::SetLevel(level_sub::AF_LEVEL, 128)
            .to_frame()
            .unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(
            bytes,
            vec![0xFE, 0xFE, 0xB4, 0xE0, 0x14, 0x01, 0x01, 0x28, 0xFD]
        );
    }

    #[test]
    fn test_read_s_meter() {
        let frame = Command::ReadMeter(meter_sub::S_METER).to_frame().unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![0xFE, 0xFE, 0xB4, 0xE0, 0x15, 0x02, 0xFD]);
    }

    #[test]
    fn test_power_on() {
        let frame = Command::PowerOn.to_frame().unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![0xFE, 0xFE, 0xB4, 0xE0, 0x18, 0x01, 0xFD]);
    }

    #[test]
    fn test_read_transceiver_id() {
        let frame = Command::ReadTransceiverId.to_frame().unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![0xFE, 0xFE, 0xB4, 0xE0, 0x19, 0x00, 0xFD]);
    }

    #[test]
    fn test_command_byte() {
        assert_eq!(Command::ReadFrequency.command_byte(), 0x03);
        assert_eq!(Command::ReadMode.command_byte(), 0x04);
        assert_eq!(
            Command::SetFrequency(Frequency::from_hz(0).unwrap()).command_byte(),
            0x05
        );
        assert_eq!(Command::SetMode(OperatingMode::Fm).command_byte(), 0x06);
    }
}
