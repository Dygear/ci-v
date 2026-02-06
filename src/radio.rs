use std::io::{Read, Write};
use std::time::{Duration, Instant};

use log::{debug, trace, warn};

use crate::command::{level_sub, meter_sub, Command};
use crate::error::{CivError, Result};
use crate::frequency::Frequency;
use crate::mode::OperatingMode;
use crate::port;
use crate::protocol::{Frame, ADDR_CONTROLLER, ADDR_ID52};
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
    /// Whether USB echo-back is enabled.
    /// USB serial mode echoes sent commands before the response.
    pub echo_back: bool,
}

impl Default for RadioConfig {
    fn default() -> Self {
        Self {
            radio_addr: ADDR_ID52,
            controller_addr: ADDR_CONTROLLER,
            baud_rate: 19200,
            timeout: Duration::from_millis(1000),
            echo_back: true,
        }
    }
}

/// A connection to an ICOM radio via CI-V protocol.
pub struct Radio {
    port: Box<dyn serialport::SerialPort>,
    config: RadioConfig,
    /// Internal read buffer to handle partial reads.
    buf: Vec<u8>,
}

impl Radio {
    /// Create a new `Radio` from an already-opened serial port and config.
    pub fn new(port: Box<dyn serialport::SerialPort>, config: RadioConfig) -> Self {
        Self {
            port,
            config,
            buf: Vec::with_capacity(256),
        }
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

        // If echo-back is enabled, read and discard the echo first.
        if self.config.echo_back {
            self.read_echo(&frame)?;
        }

        // Read the actual response.
        let response_frame = self.read_response()?;
        response::parse_response(&response_frame, command)
    }

    /// Read and discard the echo-back frame.
    fn read_echo(&mut self, sent_frame: &Frame) -> Result<()> {
        let deadline = Instant::now() + self.config.timeout;

        loop {
            self.fill_buf(deadline)?;

            if let Some((frame, consumed)) = Frame::parse(&self.buf)? {
                self.buf.drain(..consumed);
                // The echo is a frame addressed to the radio (dst = radio_addr).
                if frame.dst == self.config.radio_addr {
                    trace!("echo: {:02X?}", sent_frame.to_bytes());
                    return Ok(());
                }
                // If we got something else, keep reading (could be buffered data).
                debug!("unexpected frame while waiting for echo: {:?}", frame);
            }

            if Instant::now() >= deadline {
                warn!("timeout waiting for echo");
                return Err(CivError::Timeout);
            }
        }
    }

    /// Read a response frame from the radio (addressed to the controller).
    fn read_response(&mut self) -> Result<Frame> {
        let deadline = Instant::now() + self.config.timeout;

        loop {
            self.fill_buf(deadline)?;

            if let Some((frame, consumed)) = Frame::parse(&self.buf)? {
                self.buf.drain(..consumed);
                if frame.dst == self.config.controller_addr {
                    trace!("RX: {:?}", frame);
                    return Ok(frame);
                }
                // Skip echo frames that slipped through.
                debug!("skipping frame not addressed to controller: {:?}", frame);
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
        let _ = self.port.set_timeout(remaining.min(Duration::from_millis(100)));

        let mut tmp = [0u8; 128];
        match self.port.read(&mut tmp) {
            Ok(n) => {
                trace!("read {} bytes: {:02X?}", n, &tmp[..n]);
                self.buf.extend_from_slice(&tmp[..n]);
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
}
