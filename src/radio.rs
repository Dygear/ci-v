use std::io::{Read, Write};
use std::time::{Duration, Instant};

use log::{trace, warn};

use crate::command::{Command, level_sub, meter_sub, tone_sub, various_sub};
use crate::error::{CivError, Result};
use crate::frequency::Frequency;
use crate::mode::OperatingMode;
use crate::port;
use crate::protocol::{ADDR_CONTROLLER, ADDR_ID52, Frame};
use crate::response::{self, Response};

/// Configuration for the radio connection.
#[derive(Debug, Clone)]
pub struct RadioConfig {
    /// CI-V address of the radio.
    pub radio_addr: u8,
    /// CI-V address of the controller (this PC).
    pub controller_addr: u8,
    /// Serial baud rate.
    pub baud_rate: u32,
    /// Timeout for waiting for a response.
    pub timeout: Duration,
}

impl Default for RadioConfig {
    fn default() -> Self {
        Self {
            radio_addr: ADDR_ID52,
            controller_addr: ADDR_CONTROLLER,
            baud_rate: 19200,
            timeout: Duration::from_millis(1000),
        }
    }
}

/// A connection to an ICOM radio via CI-V protocol.
pub struct Radio {
    port: Box<dyn serialport::SerialPort>,
    config: RadioConfig,
    /// Internal read buffer to handle partial reads.
    buf: Vec<u8>,
    /// Cumulative bytes written to the serial port.
    tx_bytes: u64,
    /// Cumulative bytes read from the serial port.
    rx_bytes: u64,
}

impl Radio {
    /// Create a new `Radio` from an already-opened serial port and config.
    pub fn new(port: Box<dyn serialport::SerialPort>, config: RadioConfig) -> Self {
        Self {
            port,
            config,
            buf: Vec::with_capacity(256),
            tx_bytes: 0,
            rx_bytes: 0,
        }
    }

    /// Return the baud rate of the current connection.
    pub fn baud_rate(&self) -> u32 {
        self.config.baud_rate
    }

    /// Return cumulative bytes transmitted.
    pub fn tx_bytes(&self) -> u64 {
        self.tx_bytes
    }

    /// Return cumulative bytes received.
    pub fn rx_bytes(&self) -> u64 {
        self.rx_bytes
    }

    /// Auto-discover the ID-52A Plus and connect.
    ///
    /// Finds the port, auto-detects the baud rate, and returns a ready-to-use `Radio`.
    pub fn auto_connect() -> Result<Self> {
        let port_name = port::find_id52_port()?;
        let (baud_rate, port) = port::auto_detect_baud(&port_name)?;

        let config = RadioConfig {
            baud_rate,
            ..RadioConfig::default()
        };

        Ok(Self::new(port, config))
    }

    /// Send a command and wait for the response.
    pub fn send_command(&mut self, command: &Command) -> Result<Response> {
        let frame = command.to_frame()?;
        let bytes = frame.to_bytes();

        trace!("TX: {:02X?}", bytes);
        self.port.write_all(&bytes).map_err(CivError::Io)?;
        self.port.flush().map_err(CivError::Io)?;
        self.tx_bytes += bytes.len() as u64;

        // Read the actual response, skipping echo-back and unsolicited frames.
        let response_frame = self.read_response(command.command_byte())?;
        response::parse_response(&response_frame, command)
    }

    /// Read a response frame from the radio (addressed to the controller).
    ///
    /// Transparently skips:
    /// - Echo-back frames (addressed to the radio, not the controller)
    /// - Unsolicited transceive notifications (command byte doesn't match
    ///   the expected response and isn't OK/NG)
    ///
    /// This ensures that when CI-V Transceive is ON, unsolicited frequency
    /// or mode change notifications don't get mistaken for command responses.
    fn read_response(&mut self, expected_cmd: u8) -> Result<Frame> {
        let deadline = Instant::now() + self.config.timeout;

        loop {
            self.fill_buf(deadline)?;

            if let Some((frame, consumed)) = Frame::parse(&self.buf)? {
                self.buf.drain(..consumed);

                if frame.dst != self.config.controller_addr {
                    // Skip echo-back frames (addressed to the radio, not to us).
                    trace!("skipping echo frame: {:?}", frame);
                    continue;
                }

                if frame.is_ok() || frame.is_ng() || frame.command == expected_cmd {
                    trace!("RX: {:?}", frame);
                    return Ok(frame);
                }

                // Unsolicited transceive notification — skip it.
                trace!(
                    "skipping unsolicited frame (cmd {:02X}, expected {:02X}): {:?}",
                    frame.command, expected_cmd, frame
                );
            }

            if Instant::now() >= deadline {
                return Err(CivError::Timeout);
            }
        }
    }

    /// Read data from the serial port into the internal buffer.
    fn fill_buf(&mut self, deadline: Instant) -> Result<()> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(CivError::Timeout);
        }

        // Set the timeout for this read.
        let _ = self
            .port
            .set_timeout(remaining.min(Duration::from_millis(100)));

        let mut tmp = [0u8; 128];
        match self.port.read(&mut tmp) {
            Ok(n) => {
                trace!("read {} bytes: {:02X?}", n, &tmp[..n]);
                self.buf.extend_from_slice(&tmp[..n]);
                self.rx_bytes += n as u64;
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(()),
            Err(e) => Err(CivError::Io(e)),
        }
    }

    // --- Convenience methods ---

    /// Read the current operating frequency.
    pub fn read_frequency(&mut self) -> Result<Frequency> {
        match self.send_command(&Command::ReadFrequency)? {
            Response::Frequency(f) => Ok(f),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadFrequency: {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Set the operating frequency.
    pub fn set_frequency(&mut self, freq: Frequency) -> Result<()> {
        match self.send_command(&Command::SetFrequency(freq))? {
            Response::Ok => Ok(()),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to SetFrequency: {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read the current operating mode.
    pub fn read_mode(&mut self) -> Result<OperatingMode> {
        match self.send_command(&Command::ReadMode)? {
            Response::Mode(m) => Ok(m),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadMode: {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Set the operating mode.
    pub fn set_mode(&mut self, mode: OperatingMode) -> Result<()> {
        match self.send_command(&Command::SetMode(mode))? {
            Response::Ok => Ok(()),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to SetMode: {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read the S-meter level (0–255).
    pub fn read_s_meter(&mut self) -> Result<u16> {
        match self.send_command(&Command::ReadMeter(meter_sub::S_METER))? {
            Response::Meter(_, v) => Ok(v),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadMeter(S): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read the AF (volume) level (0–255).
    pub fn read_af_level(&mut self) -> Result<u16> {
        match self.send_command(&Command::ReadLevel(level_sub::AF_LEVEL))? {
            Response::Level(_, v) => Ok(v),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadLevel(AF): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Set the AF (volume) level (0–255).
    pub fn set_af_level(&mut self, level: u16) -> Result<()> {
        match self.send_command(&Command::SetLevel(level_sub::AF_LEVEL, level))? {
            Response::Ok => Ok(()),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to SetLevel(AF): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Set the squelch level (0–255).
    pub fn set_squelch(&mut self, level: u16) -> Result<()> {
        match self.send_command(&Command::SetLevel(level_sub::SQUELCH, level))? {
            Response::Ok => Ok(()),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to SetLevel(SQL): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read the squelch level (0–255).
    pub fn read_squelch(&mut self) -> Result<u16> {
        match self.send_command(&Command::ReadLevel(level_sub::SQUELCH))? {
            Response::Level(_, v) => Ok(v),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadLevel(SQL): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Select VFO A.
    pub fn select_vfo_a(&mut self) -> Result<()> {
        match self.send_command(&Command::SelectVfoA)? {
            Response::Ok => Ok(()),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to SelectVfoA: {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Select VFO B.
    pub fn select_vfo_b(&mut self) -> Result<()> {
        match self.send_command(&Command::SelectVfoB)? {
            Response::Ok => Ok(()),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to SelectVfoB: {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read the RF power level (0–255).
    pub fn read_rf_power(&mut self) -> Result<u16> {
        match self.send_command(&Command::ReadLevel(level_sub::RF_POWER))? {
            Response::Level(_, v) => Ok(v),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadLevel(RF_POWER): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read a various function setting. Returns the raw byte value.
    pub fn read_various(&mut self, sub: u8) -> Result<u8> {
        match self.send_command(&Command::ReadVarious(sub))? {
            Response::Various(_, v) => Ok(v),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadVarious: {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read the tone squelch function (0x00–0x09).
    pub fn read_tone_mode(&mut self) -> Result<u8> {
        self.read_various(various_sub::TONE_SQUELCH_FUNC)
    }

    /// Read the duplex direction (0x10=Simplex, 0x11=DUP-, 0x12=DUP+).
    pub fn read_duplex(&mut self) -> Result<u8> {
        match self.send_command(&Command::ReadDuplex)? {
            Response::Duplex(d) => Ok(d),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadDuplex: {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read the duplex offset frequency.
    pub fn read_offset(&mut self) -> Result<Frequency> {
        match self.send_command(&Command::ReadOffset)? {
            Response::Offset(f) => Ok(f),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadOffset: {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read the repeater tone (Tx) frequency in tenths of Hz.
    pub fn read_tx_tone(&mut self) -> Result<u16> {
        match self.send_command(&Command::ReadTone(tone_sub::REPEATER_TONE))? {
            Response::ToneFrequency(_, f) => Ok(f),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadTone(Tx): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read the TSQL tone (Rx) frequency in tenths of Hz.
    pub fn read_rx_tone(&mut self) -> Result<u16> {
        match self.send_command(&Command::ReadTone(tone_sub::TSQL_TONE))? {
            Response::ToneFrequency(_, f) => Ok(f),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadTone(Rx): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Read the DTCS code and polarity. Returns (tx_polarity, rx_polarity, code).
    pub fn read_dtcs(&mut self) -> Result<(u8, u8, u16)> {
        match self.send_command(&Command::ReadTone(tone_sub::DTCS))? {
            Response::DtcsCode(tx_pol, rx_pol, code) => Ok((tx_pol, rx_pol, code)),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to ReadTone(DTCS): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Set the tone/squelch function mode (0x00–0x09).
    pub fn set_tone_mode(&mut self, mode: u8) -> Result<()> {
        match self.send_command(&Command::SetVarious(various_sub::TONE_SQUELCH_FUNC, mode))? {
            Response::Ok => Ok(()),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to SetVarious(ToneMode): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Set the repeater tone (Tx) frequency in tenths of Hz.
    pub fn set_tx_tone(&mut self, freq_tenths: u16) -> Result<()> {
        match self.send_command(&Command::SetTone(tone_sub::REPEATER_TONE, freq_tenths))? {
            Response::Ok => Ok(()),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to SetTone(Tx): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Set the TSQL tone (Rx) frequency in tenths of Hz.
    pub fn set_rx_tone(&mut self, freq_tenths: u16) -> Result<()> {
        match self.send_command(&Command::SetTone(tone_sub::TSQL_TONE, freq_tenths))? {
            Response::Ok => Ok(()),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to SetTone(Rx): {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }

    /// Set the DTCS code and polarity.
    pub fn set_dtcs(&mut self, tx_pol: u8, rx_pol: u8, code: u16) -> Result<()> {
        match self.send_command(&Command::SetDtcs(tx_pol, rx_pol, code))? {
            Response::Ok => Ok(()),
            Response::Ng => Err(CivError::Ng),
            other => {
                warn!("unexpected response to SetDtcs: {:?}", other);
                Err(CivError::InvalidFrame)
            }
        }
    }
}
