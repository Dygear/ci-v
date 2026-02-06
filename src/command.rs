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
    /// Power on/off control.
    pub const POWER: u8 = 0x18;
    /// Read transceiver ID.
    pub const READ_ID: u8 = 0x19;
}

/// Sub-commands for the LEVEL (0x14) command.
pub mod level_sub {
    /// AF output level (volume).
    pub const AF_LEVEL: u8 = 0x01;
    /// RF gain level.
    pub const RF_GAIN: u8 = 0x02;
    /// Squelch level.
    pub const SQUELCH: u8 = 0x03;
}

/// Sub-commands for the METER (0x15) command.
pub mod meter_sub {
    /// S-meter reading.
    pub const S_METER: u8 = 0x02;
    /// Power meter reading.
    pub const POWER_METER: u8 = 0x11;
}

/// Sub-commands for the VFO_MODE (0x07) command.
pub mod vfo_sub {
    /// Select VFO A.
    pub const VFO_A: u8 = 0x00;
    /// Select VFO B.
    pub const VFO_B: u8 = 0x01;
    /// Exchange main/sub (A/B swap).
    pub const EXCHANGE: u8 = 0xB0;
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
    /// Select VFO A.
    SelectVfoA,
    /// Select VFO B.
    SelectVfoB,
    /// Exchange VFO A/B.
    ExchangeVfo,
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
            Command::ExchangeVfo => Frame::new(cmd::VFO_MODE, Some(vfo_sub::EXCHANGE), vec![]),
            Command::ReadLevel(sub) => Frame::new(cmd::LEVEL, Some(*sub), vec![]),
            Command::SetLevel(sub, value) => {
                let data = bcd::encode_bcd_be(*value as u64, 2)?;
                Frame::new(cmd::LEVEL, Some(*sub), data)
            }
            Command::ReadMeter(sub) => Frame::new(cmd::METER, Some(*sub), vec![]),
            Command::PowerOn => Frame::new(cmd::POWER, Some(power_sub::ON), vec![]),
            Command::PowerOff => Frame::new(cmd::POWER, Some(power_sub::OFF), vec![]),
            Command::ReadTransceiverId => Frame::new(cmd::READ_ID, Some(0x00), vec![]),
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
            Command::SelectVfoA | Command::SelectVfoB | Command::ExchangeVfo => cmd::VFO_MODE,
            Command::ReadLevel(_) | Command::SetLevel(_, _) => cmd::LEVEL,
            Command::ReadMeter(_) => cmd::METER,
            Command::PowerOn | Command::PowerOff => cmd::POWER,
            Command::ReadTransceiverId => cmd::READ_ID,
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
            Command::ExchangeVfo => Some(vfo_sub::EXCHANGE),
            Command::ReadLevel(sub) | Command::SetLevel(sub, _) => Some(*sub),
            Command::ReadMeter(sub) => Some(*sub),
            Command::PowerOn => Some(power_sub::ON),
            Command::PowerOff => Some(power_sub::OFF),
            Command::ReadTransceiverId => Some(0x00),
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
            vec![0xFE, 0xFE, 0xB4, 0xE0, 0x05, 0x00, 0x00, 0x00, 0x45, 0x01, 0xFD]
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
        assert_eq!(bytes, vec![0xFE, 0xFE, 0xB4, 0xE0, 0x07, 0x00, 0xFD]);
    }

    #[test]
    fn test_read_af_level() {
        let frame = Command::ReadLevel(level_sub::AF_LEVEL).to_frame().unwrap();
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![0xFE, 0xFE, 0xB4, 0xE0, 0x14, 0x01, 0xFD]);
    }

    #[test]
    fn test_set_af_level() {
        let frame = Command::SetLevel(level_sub::AF_LEVEL, 128).to_frame().unwrap();
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
