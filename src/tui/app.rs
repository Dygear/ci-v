use std::sync::mpsc as std_mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::frequency::Frequency;
use crate::mode::OperatingMode;

use super::message::{RadioCommand, RadioEvent, RadioState, Vfo};

/// Which field is focused for editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Frequency,
    Mode,
    AfLevel,
    Squelch,
    Vfo,
}

/// Current input mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing(Focus),
}

/// Hz step for each cursor position in the 9-digit frequency display (NNN.NNN.NNN).
/// Position 0 is the leftmost (100 MHz), position 8 is the rightmost (1 Hz).
const FREQ_DIGIT_POWERS: [u64; 9] = [
    100_000_000, // pos 0: 100 MHz
    10_000_000,  // pos 1: 10 MHz
    1_000_000,   // pos 2: 1 MHz
    100_000,     // pos 3: 100 kHz
    10_000,      // pos 4: 10 kHz
    1_000,       // pos 5: 1 kHz
    100,         // pos 6: 100 Hz
    10,          // pos 7: 10 Hz
    1,           // pos 8: 1 Hz
];

/// Maximum volume step on the radio (0–39).
const VOLUME_MAX_STEP: u16 = 39;

/// Convert a volume step (0–39) to the raw CI-V value (3–252).
/// Step 0 → 3, Step 1 → 9, Step 2 → 16, ..., Step 39 → 252.
pub fn volume_step_to_raw(step: u16) -> u16 {
    let step = step.min(VOLUME_MAX_STEP);
    (3.0 + step as f64 * 249.0 / VOLUME_MAX_STEP as f64).round() as u16
}

/// Convert a raw CI-V value (0–255) to the nearest volume step (0–39).
pub fn raw_to_volume_step(raw: u16) -> u16 {
    if raw <= 3 {
        return 0;
    }
    let step = ((raw as f64 - 3.0) * VOLUME_MAX_STEP as f64 / 249.0).round() as u16;
    step.min(VOLUME_MAX_STEP)
}

/// All modes in cycle order.
const MODE_CYCLE: [OperatingMode; 5] = [
    OperatingMode::Fm,
    OperatingMode::FmN,
    OperatingMode::Am,
    OperatingMode::AmN,
    OperatingMode::Dv,
];

/// Application state.
pub struct App {
    pub radio_state: RadioState,
    pub input_mode: InputMode,
    pub connected: bool,
    pub last_error: Option<String>,
    pub should_quit: bool,
    pub baud_rate: u32,

    // Edit buffers
    pub freq_edit_hz: u64,
    pub freq_cursor: usize,
    pub mode_edit: OperatingMode,
    pub af_edit: u16,
    pub sql_edit: u16,
    pub vfo_edit: Vfo,

    /// When muted, stores the volume step to restore on unmute.
    pub mute_restore_step: Option<u16>,

    cmd_tx: std_mpsc::Sender<RadioCommand>,
}

impl App {
    pub fn new(cmd_tx: std_mpsc::Sender<RadioCommand>, baud_rate: u32) -> Self {
        Self {
            radio_state: RadioState::default(),
            input_mode: InputMode::Normal,
            connected: false,
            last_error: None,
            should_quit: false,
            baud_rate,
            freq_edit_hz: 145_000_000,
            freq_cursor: 0,
            mode_edit: OperatingMode::Fm,
            af_edit: 0,
            sql_edit: 0,
            vfo_edit: Vfo::A,
            mute_restore_step: None,
            cmd_tx,
        }
    }

    /// Handle a radio event from the radio task.
    pub fn handle_radio_event(&mut self, event: RadioEvent) {
        match event {
            RadioEvent::StateUpdate(state) => {
                // If muted but the radio reports a non-zero volume (user changed
                // it on the device), clear the mute state.
                if self.mute_restore_step.is_some() {
                    if let Some(raw) = state.af_level {
                        if raw_to_volume_step(raw) != 0 {
                            self.mute_restore_step = None;
                        }
                    }
                }
                self.radio_state = state;
                self.last_error = None;
            }
            RadioEvent::Error(msg) => {
                self.last_error = Some(msg);
            }
            RadioEvent::Connected => {
                self.connected = true;
            }
            RadioEvent::Disconnected => {
                self.connected = false;
            }
        }
    }

    /// Handle a key event.
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl+C always quits.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit();
            return;
        }

        match self.input_mode {
            InputMode::Normal => self.handle_normal_key(key),
            InputMode::Editing(focus) => self.handle_edit_key(key, focus),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.quit(),
            KeyCode::Char('f') | KeyCode::Char('F') => self.enter_edit(Focus::Frequency),
            KeyCode::Char('m') | KeyCode::Char('M') => self.enter_edit(Focus::Mode),
            KeyCode::Char('a') | KeyCode::Char('A') => self.enter_edit(Focus::AfLevel),
            KeyCode::Char('s') | KeyCode::Char('S') => self.enter_edit(Focus::Squelch),
            KeyCode::Char('v') | KeyCode::Char('V') => self.enter_edit(Focus::Vfo),
            KeyCode::Char('+') | KeyCode::Char('=') => self.adjust_volume(1),
            KeyCode::Char('-') | KeyCode::Char('_') => self.adjust_volume(-1),
            KeyCode::Char('0') => self.toggle_mute(),
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                self.enter_edit(Focus::Frequency);
                self.handle_freq_edit_key(key.code);
            }
            _ => {}
        }
    }

    fn handle_edit_key(&mut self, key: KeyEvent, focus: Focus) {
        // Pressing the same hotkey that entered edit mode cancels without saving.
        let cancel_key = matches!(
            (key.code, focus),
            (KeyCode::Char('f') | KeyCode::Char('F'), Focus::Frequency)
                | (KeyCode::Char('m') | KeyCode::Char('M'), Focus::Mode)
                | (KeyCode::Char('a') | KeyCode::Char('A'), Focus::AfLevel)
                | (KeyCode::Char('s') | KeyCode::Char('S'), Focus::Squelch)
                | (KeyCode::Char('v') | KeyCode::Char('V'), Focus::Vfo)
        );

        match key.code {
            _ if cancel_key => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Enter => {
                self.confirm_edit(focus);
                self.input_mode = InputMode::Normal;
            }
            _ => match focus {
                Focus::Frequency => self.handle_freq_edit_key(key.code),
                Focus::Mode => self.handle_mode_edit_key(key.code),
                Focus::AfLevel => self.handle_volume_edit_key(key.code),
                Focus::Squelch => self.handle_level_edit_key(key.code),
                Focus::Vfo => self.handle_vfo_edit_key(key.code),
            },
        }
    }

    fn enter_edit(&mut self, focus: Focus) {
        match focus {
            Focus::Frequency => {
                self.freq_edit_hz = self
                    .radio_state
                    .frequency
                    .map(|f| f.hz())
                    .unwrap_or(145_000_000);
                self.freq_cursor = 0;
            }
            Focus::Mode => {
                self.mode_edit = self.radio_state.mode.unwrap_or(OperatingMode::Fm);
            }
            Focus::AfLevel => {
                self.af_edit = raw_to_volume_step(self.radio_state.af_level.unwrap_or(3));
            }
            Focus::Squelch => {
                self.sql_edit = self.radio_state.squelch.unwrap_or(0);
            }
            Focus::Vfo => {
                self.vfo_edit = Vfo::A;
            }
        }
        self.input_mode = InputMode::Editing(focus);
    }

    fn confirm_edit(&mut self, focus: Focus) {
        let cmd = match focus {
            Focus::Frequency => {
                if let Ok(freq) = Frequency::from_hz(self.freq_edit_hz) {
                    RadioCommand::SetFrequency(freq)
                } else {
                    return;
                }
            }
            Focus::Mode => RadioCommand::SetMode(self.mode_edit),
            Focus::AfLevel => RadioCommand::SetAfLevel(volume_step_to_raw(self.af_edit)),
            Focus::Squelch => RadioCommand::SetSquelch(self.sql_edit),
            Focus::Vfo => RadioCommand::SelectVfo(self.vfo_edit),
        };
        let _ = self.cmd_tx.send(cmd);
    }

    fn handle_freq_edit_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Left => {
                if self.freq_cursor > 0 {
                    self.freq_cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.freq_cursor < 8 {
                    self.freq_cursor += 1;
                }
            }
            KeyCode::Up => {
                let step = FREQ_DIGIT_POWERS[self.freq_cursor];
                let new_hz = self.freq_edit_hz.saturating_add(step);
                if new_hz <= 9_999_999_999 {
                    self.freq_edit_hz = new_hz;
                }
            }
            KeyCode::Down => {
                let step = FREQ_DIGIT_POWERS[self.freq_cursor];
                self.freq_edit_hz = self.freq_edit_hz.saturating_sub(step);
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let digit = c as u64 - b'0' as u64;
                let power = FREQ_DIGIT_POWERS[self.freq_cursor];
                // Replace the digit at the current cursor position.
                let current_digit = (self.freq_edit_hz / power) % 10;
                let new_hz = self.freq_edit_hz - current_digit * power + digit * power;
                if new_hz <= 9_999_999_999 {
                    self.freq_edit_hz = new_hz;
                    // Auto-advance cursor.
                    if self.freq_cursor < 8 {
                        self.freq_cursor += 1;
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_mode_edit_key(&mut self, code: KeyCode) {
        let idx = MODE_CYCLE.iter().position(|m| *m == self.mode_edit).unwrap_or(0);
        match code {
            KeyCode::Left | KeyCode::Up => {
                let new_idx = if idx == 0 { MODE_CYCLE.len() - 1 } else { idx - 1 };
                self.mode_edit = MODE_CYCLE[new_idx];
            }
            KeyCode::Right | KeyCode::Down => {
                let new_idx = (idx + 1) % MODE_CYCLE.len();
                self.mode_edit = MODE_CYCLE[new_idx];
            }
            _ => {}
        }
    }

    fn handle_volume_edit_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up | KeyCode::Right => {
                if self.af_edit < VOLUME_MAX_STEP {
                    self.af_edit += 1;
                }
            }
            KeyCode::Down | KeyCode::Left => {
                if self.af_edit > 0 {
                    self.af_edit -= 1;
                }
            }
            _ => {}
        }
    }

    fn handle_level_edit_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up | KeyCode::Right => {
                if self.sql_edit < 255 {
                    self.sql_edit += 1;
                }
            }
            KeyCode::Down | KeyCode::Left => {
                if self.sql_edit > 0 {
                    self.sql_edit -= 1;
                }
            }
            _ => {}
        }
    }

    fn handle_vfo_edit_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                self.vfo_edit = self.vfo_edit.toggle();
            }
            _ => {}
        }
    }

    /// Adjust volume by `delta` steps and send immediately. Clears mute state.
    fn adjust_volume(&mut self, delta: i16) {
        let current = self
            .radio_state
            .af_level
            .map(raw_to_volume_step)
            .unwrap_or(0);
        let new_step = (current as i16 + delta).clamp(0, VOLUME_MAX_STEP as i16) as u16;
        self.mute_restore_step = None;
        let _ = self.cmd_tx.send(RadioCommand::SetAfLevel(volume_step_to_raw(new_step)));
    }

    /// Toggle mute. Muting saves the current step and sets volume to 0.
    /// Unmuting restores the saved step.
    fn toggle_mute(&mut self) {
        if let Some(restore) = self.mute_restore_step.take() {
            // Unmute: restore previous volume.
            let _ = self.cmd_tx.send(RadioCommand::SetAfLevel(volume_step_to_raw(restore)));
        } else {
            // Mute: save current volume, set to 0.
            let current = self
                .radio_state
                .af_level
                .map(raw_to_volume_step)
                .unwrap_or(0);
            self.mute_restore_step = Some(current);
            let _ = self.cmd_tx.send(RadioCommand::SetAfLevel(volume_step_to_raw(0)));
        }
    }

    fn quit(&mut self) {
        let _ = self.cmd_tx.send(RadioCommand::Quit);
        self.should_quit = true;
    }

    /// Get the 9 digits of the frequency for display.
    pub fn freq_digits(&self, hz: u64) -> [u8; 9] {
        let mut digits = [0u8; 9];
        for (i, &power) in FREQ_DIGIT_POWERS.iter().enumerate() {
            digits[i] = ((hz / power) % 10) as u8;
        }
        digits
    }
}
