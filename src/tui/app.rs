use std::sync::mpsc as std_mpsc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::frequency::Frequency;
use crate::mode::OperatingMode;

use super::message::{RadioCommand, RadioEvent, RadioState, Vfo, VfoState};

/// Which field is focused for editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Frequency,
    Mode,
    AfLevel,
    Squelch,
    TxTone,
    RxTone,
    Power,
}

/// Tone type category for the first phase of tone editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneType {
    Csq,
    Tpl,
    Dpl,
}

impl std::fmt::Display for ToneType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Csq => write!(f, "CSQ"),
            Self::Tpl => write!(f, "TPL"),
            Self::Dpl => write!(f, "DPL"),
        }
    }
}

/// Editing phase for tone selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneEditPhase {
    /// Selecting between CSQ / TPL / DPL.
    SelectType,
    /// Selecting the specific tone freq or DTCS code.
    SelectValue,
}

/// Standard CTCSS tones in tenths of Hz.
pub const CTCSS_TONES: &[u16] = &[
    670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148, 1188,
    1230, 1273, 1318, 1365, 1413, 1462, 1514, 1567, 1622, 1679, 1738, 1799, 1862, 1928, 2035, 2065,
    2107, 2181, 2257, 2291, 2336, 2418, 2503, 2541,
];

/// Standard DTCS codes.
pub const DTCS_CODES: &[u16] = &[
    23, 25, 26, 31, 32, 36, 43, 47, 51, 53, 54, 65, 71, 72, 73, 74, 114, 115, 116, 122, 125, 131,
    132, 134, 143, 145, 152, 155, 156, 162, 165, 172, 174, 205, 212, 223, 225, 226, 243, 244, 245,
    246, 251, 252, 255, 261, 263, 265, 266, 271, 274, 306, 311, 315, 325, 331, 332, 343, 346, 351,
    356, 364, 365, 371, 411, 412, 413, 423, 431, 432, 445, 446, 452, 454, 455, 462, 464, 465, 466,
    503, 506, 516, 523, 526, 532, 546, 565, 606, 612, 624, 627, 631, 632, 654, 662, 664, 703, 712,
    723, 731, 732, 734, 743, 754,
];

/// RF power level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerLevel {
    SLow,
    Low1,
    Low2,
    Mid,
    High,
}

impl PowerLevel {
    /// All levels in order from lowest to highest.
    pub const ALL: [PowerLevel; 5] = [
        PowerLevel::SLow,
        PowerLevel::Low1,
        PowerLevel::Low2,
        PowerLevel::Mid,
        PowerLevel::High,
    ];

    /// Raw CI-V value (midpoint of the range) for this power level.
    pub fn to_raw(self) -> u16 {
        match self {
            Self::SLow => 0,
            Self::Low1 => 76,
            Self::Low2 => 127,
            Self::Mid => 179,
            Self::High => 255,
        }
    }

    /// Determine power level from a raw CI-V value.
    pub fn from_raw(raw: u16) -> Self {
        match raw {
            0..=50 => Self::SLow,
            51..=101 => Self::Low1,
            102..=153 => Self::Low2,
            154..=204 => Self::Mid,
            _ => Self::High,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SLow => "SLO",
            Self::Low1 => "LO1",
            Self::Low2 => "LO2",
            Self::Mid => "MID",
            Self::High => "MAX",
        }
    }

    fn cycle_up(self) -> Self {
        match self {
            Self::SLow => Self::Low1,
            Self::Low1 => Self::Low2,
            Self::Low2 => Self::Mid,
            Self::Mid => Self::High,
            Self::High => Self::High,
        }
    }

    fn cycle_down(self) -> Self {
        match self {
            Self::SLow => Self::SLow,
            Self::Low1 => Self::SLow,
            Self::Low2 => Self::Low1,
            Self::Mid => Self::Low2,
            Self::High => Self::Mid,
        }
    }
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
    pub error_log: Vec<(Instant, String)>,
    pub should_quit: bool,
    pub baud_rate: u32,

    /// Currently selected VFO (tracked locally since CI-V has no read command for this).
    pub current_vfo: Vfo,

    // Edit buffers
    pub freq_edit_hz: u64,
    pub freq_cursor: usize,
    pub mode_edit: OperatingMode,
    pub af_edit: u16,
    pub sql_edit: u16,

    /// When muted, stores the volume step to restore on unmute.
    pub mute_restore_step: Option<u16>,

    // Power edit state
    pub power_edit: PowerLevel,

    // Tone edit state
    pub tone_edit_phase: ToneEditPhase,
    pub tone_type_edit: ToneType,
    pub tone_freq_edit: usize,
    pub dtcs_code_edit: usize,
    pub dtcs_pol_edit: bool,

    cmd_tx: std_mpsc::Sender<RadioCommand>,
}

impl App {
    pub fn new(cmd_tx: std_mpsc::Sender<RadioCommand>, baud_rate: u32) -> Self {
        Self {
            radio_state: RadioState::default(),
            input_mode: InputMode::Normal,
            connected: false,
            error_log: Vec::new(),
            should_quit: false,
            baud_rate,
            current_vfo: Vfo::A,
            freq_edit_hz: 145_000_000,
            freq_cursor: 0,
            mode_edit: OperatingMode::Fm,
            af_edit: 0,
            sql_edit: 0,
            mute_restore_step: None,
            power_edit: PowerLevel::Mid,
            tone_edit_phase: ToneEditPhase::SelectType,
            tone_type_edit: ToneType::Csq,
            tone_freq_edit: 0,
            dtcs_code_edit: 0,
            dtcs_pol_edit: false,
            cmd_tx,
        }
    }

    /// Handle a radio event from the radio task.
    pub fn handle_radio_event(&mut self, event: RadioEvent) {
        match event {
            RadioEvent::StateUpdate(state) => {
                // If muted but the radio reports a non-zero volume (user changed
                // it on the device), clear the mute state.
                if self.mute_restore_step.is_some()
                    && let Some(raw) = state.af_level
                    && raw_to_volume_step(raw) != 0
                {
                    self.mute_restore_step = None;
                }
                self.radio_state = state;
            }
            RadioEvent::Error(msg) => {
                self.error_log.push((Instant::now(), msg));
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
            KeyCode::Char('t') | KeyCode::Char('T') => self.enter_edit(Focus::TxTone),
            KeyCode::Char('r') | KeyCode::Char('R') => self.enter_edit(Focus::RxTone),
            KeyCode::Char('p') | KeyCode::Char('P') => self.enter_edit(Focus::Power),
            KeyCode::Char('w') | KeyCode::Char('W') => self.toggle_width(),
            KeyCode::Char('v') | KeyCode::Char('V') => self.toggle_vfo(),
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
                | (KeyCode::Char('t') | KeyCode::Char('T'), Focus::TxTone)
                | (KeyCode::Char('r') | KeyCode::Char('R'), Focus::RxTone)
                | (KeyCode::Char('p') | KeyCode::Char('P'), Focus::Power)
        );

        match key.code {
            _ if cancel_key => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Esc => {
                // For tone editing, Esc in SelectValue goes back to SelectType.
                if matches!(focus, Focus::TxTone | Focus::RxTone)
                    && self.tone_edit_phase == ToneEditPhase::SelectValue
                {
                    self.tone_edit_phase = ToneEditPhase::SelectType;
                } else {
                    self.input_mode = InputMode::Normal;
                }
            }
            KeyCode::Enter => {
                if matches!(focus, Focus::TxTone | Focus::RxTone) {
                    self.handle_tone_enter(focus);
                } else {
                    self.confirm_edit(focus);
                    self.input_mode = InputMode::Normal;
                }
            }
            _ => match focus {
                Focus::Frequency => self.handle_freq_edit_key(key.code),
                Focus::Mode => self.handle_mode_edit_key(key.code),
                Focus::AfLevel => self.handle_volume_edit_key(key.code),
                Focus::Squelch => self.handle_level_edit_key(key.code),
                Focus::TxTone | Focus::RxTone => self.handle_tone_edit_key(key.code),
                Focus::Power => self.handle_power_edit_key(key.code),
            },
        }
    }

    /// Get the VfoState for the currently active VFO.
    pub fn active_vfo_state(&self) -> &VfoState {
        match self.current_vfo {
            Vfo::A => &self.radio_state.vfo_a,
            Vfo::B => &self.radio_state.vfo_b,
        }
    }

    fn enter_edit(&mut self, focus: Focus) {
        match focus {
            Focus::Frequency => {
                self.freq_edit_hz = self
                    .active_vfo_state()
                    .frequency
                    .map(|f| f.hz())
                    .unwrap_or(145_000_000);
                self.freq_cursor = 0;
            }
            Focus::Mode => {
                self.mode_edit = self.active_vfo_state().mode.unwrap_or(OperatingMode::Fm);
            }
            Focus::AfLevel => {
                self.af_edit = raw_to_volume_step(self.radio_state.af_level.unwrap_or(3));
            }
            Focus::Squelch => {
                self.sql_edit = self.radio_state.squelch.unwrap_or(0);
            }
            Focus::Power => {
                self.power_edit = self
                    .active_vfo_state()
                    .rf_power
                    .map(PowerLevel::from_raw)
                    .unwrap_or(PowerLevel::Mid);
            }
            Focus::TxTone | Focus::RxTone => {
                self.tone_edit_phase = ToneEditPhase::SelectType;
                let is_tx = focus == Focus::TxTone;
                // Copy values out of the borrow before mutating self.
                let state = self.active_vfo_state();
                let tone_mode = state.tone_mode.unwrap_or(0x00);
                let tx_freq = state.tx_tone_freq;
                let rx_freq = state.rx_tone_freq;
                let dtcs_code = state.dtcs_code;
                let dtcs_tx_pol = state.dtcs_tx_pol.unwrap_or(0);
                let dtcs_rx_pol = state.dtcs_rx_pol.unwrap_or(0);
                // Now mutate self freely.
                self.tone_type_edit = current_tone_type(tone_mode, is_tx);
                let tone_freq = if is_tx { tx_freq } else { rx_freq };
                self.tone_freq_edit = tone_freq
                    .and_then(|f| CTCSS_TONES.iter().position(|&t| t == f))
                    .unwrap_or(0);
                self.dtcs_code_edit = dtcs_code
                    .and_then(|c| DTCS_CODES.iter().position(|&d| d == c))
                    .unwrap_or(0);
                self.dtcs_pol_edit = if is_tx {
                    dtcs_tx_pol != 0
                } else {
                    dtcs_rx_pol != 0
                };
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
            Focus::Power => RadioCommand::SetRfPower(self.power_edit.to_raw()),
            Focus::TxTone | Focus::RxTone => return, // handled by confirm_tone
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
        let idx = MODE_CYCLE
            .iter()
            .position(|m| *m == self.mode_edit)
            .unwrap_or(0);
        match code {
            KeyCode::Left | KeyCode::Up => {
                let new_idx = if idx == 0 {
                    MODE_CYCLE.len() - 1
                } else {
                    idx - 1
                };
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

    fn handle_power_edit_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up | KeyCode::Right => {
                self.power_edit = self.power_edit.cycle_up();
            }
            KeyCode::Down | KeyCode::Left => {
                self.power_edit = self.power_edit.cycle_down();
            }
            _ => {}
        }
    }

    fn handle_tone_edit_key(&mut self, code: KeyCode) {
        match self.tone_edit_phase {
            ToneEditPhase::SelectType => match code {
                KeyCode::Left => {
                    self.tone_type_edit = match self.tone_type_edit {
                        ToneType::Csq => ToneType::Dpl,
                        ToneType::Tpl => ToneType::Csq,
                        ToneType::Dpl => ToneType::Tpl,
                    };
                }
                KeyCode::Right => {
                    self.tone_type_edit = match self.tone_type_edit {
                        ToneType::Csq => ToneType::Tpl,
                        ToneType::Tpl => ToneType::Dpl,
                        ToneType::Dpl => ToneType::Csq,
                    };
                }
                _ => {}
            },
            ToneEditPhase::SelectValue => match self.tone_type_edit {
                ToneType::Tpl => match code {
                    KeyCode::Up => {
                        if self.tone_freq_edit > 0 {
                            self.tone_freq_edit -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if self.tone_freq_edit < CTCSS_TONES.len() - 1 {
                            self.tone_freq_edit += 1;
                        }
                    }
                    _ => {}
                },
                ToneType::Dpl => match code {
                    KeyCode::Up => {
                        if self.dtcs_code_edit > 0 {
                            self.dtcs_code_edit -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if self.dtcs_code_edit < DTCS_CODES.len() - 1 {
                            self.dtcs_code_edit += 1;
                        }
                    }
                    KeyCode::Left | KeyCode::Right => {
                        self.dtcs_pol_edit = !self.dtcs_pol_edit;
                    }
                    _ => {}
                },
                ToneType::Csq => {}
            },
        }
    }

    fn handle_tone_enter(&mut self, focus: Focus) {
        match self.tone_edit_phase {
            ToneEditPhase::SelectType => {
                if self.tone_type_edit == ToneType::Csq {
                    // CSQ: set tone mode and done.
                    self.confirm_tone(focus);
                    self.input_mode = InputMode::Normal;
                } else {
                    // TPL or DPL: advance to value selection.
                    self.tone_edit_phase = ToneEditPhase::SelectValue;
                }
            }
            ToneEditPhase::SelectValue => {
                self.confirm_tone(focus);
                self.input_mode = InputMode::Normal;
            }
        }
    }

    fn confirm_tone(&mut self, focus: Focus) {
        let is_tx = focus == Focus::TxTone;
        let state = self.active_vfo_state();
        let current_tone_mode = state.tone_mode.unwrap_or(0x00);
        let current_tx_pol = state.dtcs_tx_pol.unwrap_or(0);
        let current_rx_pol = state.dtcs_rx_pol.unwrap_or(0);

        match self.tone_type_edit {
            ToneType::Csq => {
                // Determine the new tone_mode based on what the *other* side is doing.
                let new_mode = compute_tone_mode(current_tone_mode, is_tx, ToneType::Csq);
                let _ = self.cmd_tx.send(RadioCommand::SetToneMode(new_mode));
            }
            ToneType::Tpl => {
                let freq = CTCSS_TONES[self.tone_freq_edit];
                // Set the tone frequency first.
                if is_tx {
                    let _ = self.cmd_tx.send(RadioCommand::SetTxTone(freq));
                } else {
                    let _ = self.cmd_tx.send(RadioCommand::SetRxTone(freq));
                }
                // Then set the tone mode.
                let new_mode = compute_tone_mode(current_tone_mode, is_tx, ToneType::Tpl);
                let _ = self.cmd_tx.send(RadioCommand::SetToneMode(new_mode));
            }
            ToneType::Dpl => {
                let code = DTCS_CODES[self.dtcs_code_edit];
                let pol = if self.dtcs_pol_edit { 1u8 } else { 0u8 };
                // Set DTCS code with polarity.
                let (tx_pol, rx_pol) = if is_tx {
                    (pol, current_rx_pol)
                } else {
                    (current_tx_pol, pol)
                };
                let _ = self
                    .cmd_tx
                    .send(RadioCommand::SetDtcsCode(tx_pol, rx_pol, code));
                // Then set the tone mode.
                let new_mode = compute_tone_mode(current_tone_mode, is_tx, ToneType::Dpl);
                let _ = self.cmd_tx.send(RadioCommand::SetToneMode(new_mode));
            }
        }
    }

    /// Toggle VFO A/B and send the command immediately.
    fn toggle_vfo(&mut self) {
        self.current_vfo = self.current_vfo.toggle();
        let _ = self.cmd_tx.send(RadioCommand::SelectVfo(self.current_vfo));
    }

    /// Toggle channel width (wide ↔ narrow) and send immediately.
    fn toggle_width(&mut self) {
        if let Some(mode) = self.active_vfo_state().mode {
            let new_mode = mode.toggle_width();
            if new_mode != mode {
                let _ = self.cmd_tx.send(RadioCommand::SetMode(new_mode));
            }
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
        let _ = self
            .cmd_tx
            .send(RadioCommand::SetAfLevel(volume_step_to_raw(new_step)));
    }

    /// Toggle mute. Muting saves the current step and sets volume to 0.
    /// Unmuting restores the saved step.
    fn toggle_mute(&mut self) {
        if let Some(restore) = self.mute_restore_step.take() {
            // Unmute: restore previous volume.
            let _ = self
                .cmd_tx
                .send(RadioCommand::SetAfLevel(volume_step_to_raw(restore)));
        } else {
            // Mute: save current volume, set to 0.
            let current = self
                .radio_state
                .af_level
                .map(raw_to_volume_step)
                .unwrap_or(0);
            self.mute_restore_step = Some(current);
            let _ = self
                .cmd_tx
                .send(RadioCommand::SetAfLevel(volume_step_to_raw(0)));
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

/// Determine the current ToneType for a given side (Tx or Rx) from the tone_mode byte.
fn current_tone_type(tone_mode: u8, is_tx: bool) -> ToneType {
    if is_tx {
        match tone_mode {
            0x01 | 0x09 => ToneType::Tpl,
            0x06..=0x08 => ToneType::Dpl,
            _ => ToneType::Csq,
        }
    } else {
        match tone_mode {
            0x02 | 0x04 | 0x08 | 0x09 => ToneType::Tpl,
            0x03 | 0x05 | 0x07 => ToneType::Dpl,
            _ => ToneType::Csq,
        }
    }
}

/// Compute the new tone_mode byte when changing one side (Tx or Rx) to a new ToneType.
///
/// Tone mode mapping (Tx, Rx):
///   0x00 = (CSQ, CSQ)
///   0x01 = (TPL, CSQ)
///   0x02 = (CSQ, TPL)
///   0x03 = (CSQ, DPL)
///   0x06 = (DPL, CSQ)
///   0x07 = (DPL, DPL)
///   0x08 = (DPL, TPL)
///   0x09 = (TPL, TPL)
fn compute_tone_mode(current_mode: u8, is_tx: bool, new_type: ToneType) -> u8 {
    // First determine what the *other* side currently is.
    let other_type = current_tone_type(current_mode, !is_tx);
    let (tx, rx) = if is_tx {
        (new_type, other_type)
    } else {
        (other_type, new_type)
    };

    match (tx, rx) {
        (ToneType::Csq, ToneType::Csq) => 0x00,
        (ToneType::Tpl, ToneType::Csq) => 0x01,
        (ToneType::Csq, ToneType::Tpl) => 0x02,
        (ToneType::Csq, ToneType::Dpl) => 0x03,
        (ToneType::Dpl, ToneType::Csq) => 0x06,
        (ToneType::Dpl, ToneType::Dpl) => 0x07,
        (ToneType::Dpl, ToneType::Tpl) => 0x08,
        (ToneType::Tpl, ToneType::Tpl) => 0x09,
        // These combinations may not have direct mappings; use closest.
        (ToneType::Tpl, ToneType::Dpl) => 0x09, // fallback: TPL+TPL (radio may not support TPL+DPL)
    }
}
