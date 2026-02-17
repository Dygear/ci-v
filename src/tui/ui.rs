use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::app::{
    self, App, CTCSS_TONES, DTCS_CODES, Focus, InputMode, PowerLevel, ToneEditPhase, ToneType,
};
use super::message::{Vfo, VfoState};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Main border.
    let status = if app.connected {
        "Connected"
    } else {
        "Disconnected"
    };
    let block = Block::default()
        .title(" CI-V Controller -- ICOM ID-52A Plus ")
        .title_bottom(format!(" {status} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.connected {
            Color::Green
        } else {
            Color::Red
        }));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Layout: meters row, VFO A, VFO B, error log, help bar.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // compact meters row
            Constraint::Length(1), // VFO A row
            Constraint::Length(1), // VFO B row
            Constraint::Min(0),    // error log
            Constraint::Length(1), // help bar
        ])
        .split(inner);

    // Meters row: S-Meter, Volume, Squelch side-by-side.
    render_compact_meters(frame, app, chunks[0]);

    // VFO rows.
    let vfo_a_line = render_vfo_row(
        Vfo::A,
        &app.radio_state.vfo_a,
        app.current_vfo == Vfo::A,
        app,
    );
    frame.render_widget(Paragraph::new(vfo_a_line), chunks[1]);

    let vfo_b_line = render_vfo_row(
        Vfo::B,
        &app.radio_state.vfo_b,
        app.current_vfo == Vfo::B,
        app,
    );
    frame.render_widget(Paragraph::new(vfo_b_line), chunks[2]);

    // Error log.
    render_error_log(frame, app, chunks[3]);

    // Help bar: left-aligned help text + right-aligned stats.
    let help_area = chunks[4];
    let help_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(62)])
        .split(help_area);

    let help = render_help(app);
    frame.render_widget(Paragraph::new(help), help_chunks[0]);

    let stats = render_stats(app);
    frame.render_widget(Paragraph::new(stats), help_chunks[1]);
}

fn render_compact_meters(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(area);

    // S-Meter.
    let s_line = render_compact_meter("S", app.radio_state.s_meter, 255, Color::Green, false);
    frame.render_widget(Paragraph::new(s_line), cols[0]);

    // Volume.
    let is_editing_vol = app.input_mode == InputMode::Editing(Focus::AfLevel);
    let vol_step = if is_editing_vol {
        Some(app.af_edit)
    } else {
        app.radio_state.af_level.map(app::raw_to_volume_step)
    };
    let vol_line = render_compact_meter("Vol", vol_step, 39, Color::Cyan, is_editing_vol);
    frame.render_widget(Paragraph::new(vol_line), cols[1]);

    // Squelch.
    let is_editing_sql = app.input_mode == InputMode::Editing(Focus::Squelch);
    let sql_val = if is_editing_sql {
        Some(app.sql_edit)
    } else {
        app.radio_state.squelch
    };
    let sql_line = render_compact_meter("SQL", sql_val, 255, Color::Yellow, is_editing_sql);
    frame.render_widget(Paragraph::new(sql_line), cols[2]);
}

fn render_compact_meter(
    label: &str,
    value: Option<u16>,
    max: u16,
    color: Color,
    is_editing: bool,
) -> Line<'static> {
    let (val, display) = match value {
        Some(v) => {
            let pct = (v as u32 * 100 / max as u32) as u16;
            (v, format!("{pct:>3}%"))
        }
        None => (0, " ---%".to_string()),
    };

    let bar_width = 8;
    let filled = (val as usize * bar_width / max as usize).min(bar_width);
    let empty = bar_width - filled;

    let bar_filled: String = "\u{2588}".repeat(filled);
    let bar_empty: String = "\u{2591}".repeat(empty);

    let label_style = if is_editing {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let mut spans = vec![
        Span::styled(format!(" {label}:["), label_style),
        Span::styled(bar_filled, Style::default().fg(color)),
        Span::styled(bar_empty, Style::default().fg(Color::DarkGray)),
        Span::styled(format!("] {display}"), Style::default().fg(Color::White)),
    ];

    // Show volume as step/39 instead of percentage.
    if label == "Vol" {
        let step_display = match value {
            Some(v) => format!(" {v:>2}/39"),
            None => " --/39".to_string(),
        };
        spans.push(Span::styled(
            step_display,
            Style::default().fg(Color::DarkGray),
        ));
    }

    Line::from(spans)
}

fn render_vfo_row(vfo: Vfo, state: &VfoState, is_selected: bool, app: &App) -> Line<'static> {
    let label_style = if is_selected {
        Style::default().fg(Color::Black).bg(Color::White)
    } else {
        Style::default()
    };
    let style = Style::default();

    let editing_freq = is_selected && app.input_mode == InputMode::Editing(Focus::Frequency);
    let editing_mode = is_selected && app.input_mode == InputMode::Editing(Focus::Mode);
    let editing_tx_tone = is_selected && app.input_mode == InputMode::Editing(Focus::TxTone);
    let editing_rx_tone = is_selected && app.input_mode == InputMode::Editing(Focus::RxTone);
    let editing_power = is_selected && app.input_mode == InputMode::Editing(Focus::Power);

    // VFO label.
    let label = format!(" {vfo} ");

    // Frequency.
    let freq_str = if editing_freq {
        format_frequency(app.freq_edit_hz)
    } else {
        state
            .frequency
            .map(|f| format_frequency(f.hz()))
            .unwrap_or_else(|| "---.--.---".to_string())
    };

    // Mode.
    let mode_str = if editing_mode {
        format!("{}", app.mode_edit)
    } else {
        state
            .mode
            .map(|m| format!("{m}"))
            .unwrap_or_else(|| "---".to_string())
    };

    // Width (derived from mode).
    let width_str = if editing_mode {
        mode_width(&app.mode_edit)
    } else {
        state.mode.as_ref().map(mode_width).unwrap_or("-----")
    };

    // RF Power.
    let power_level = if editing_power {
        Some(app.power_edit)
    } else {
        state.rf_power.map(PowerLevel::from_raw)
    };

    // Tone labels with data.
    let tx_tone = if editing_tx_tone {
        tone_edit_display(app)
    } else {
        tx_tone_display(state)
    };
    let rx_tone = if editing_rx_tone {
        tone_edit_display(app)
    } else {
        rx_tone_display(state)
    };

    // Duplex + offset.
    let duplex_spans = duplex_spans(state, style);

    // Build spans — if editing freq or mode, highlight those parts.
    let mut spans: Vec<Span<'static>> = Vec::new();

    spans.push(Span::styled(
        label,
        label_style.add_modifier(Modifier::BOLD),
    ));

    if editing_freq {
        // Render frequency with cursor.
        let digits = app.freq_digits(app.freq_edit_hz);
        for (i, &d) in digits.iter().enumerate() {
            if i == 3 || i == 6 {
                spans.push(Span::styled(".", Style::default().fg(Color::DarkGray)));
            }
            let ch = format!("{d}");
            let s = if i == app.freq_cursor {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Yellow)
            };
            spans.push(Span::styled(ch, s));
        }
    } else {
        spans.push(Span::styled(format!("{freq_str:<11}"), style));
    }

    spans.push(Span::styled("  ", style));

    if editing_mode {
        spans.push(Span::styled(
            format!("{mode_str:<5}"),
            style.fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(format!("{mode_str:<5}"), style));
    }

    spans.push(Span::styled(format!(" {width_str:<6} "), style));

    let (power_str, power_color) = match power_level {
        Some(pl) => (pl.label(), power_level_color(pl)),
        None => ("---", Color::White),
    };
    let power_style = if editing_power {
        Style::default()
            .fg(Color::Black)
            .bg(power_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(power_color)
    };
    spans.push(Span::styled(format!("{power_str:<3}"), power_style));

    spans.push(Span::styled("  Tx:", style));

    let tx_tone_style = if editing_tx_tone {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        style
    };
    spans.push(Span::styled(format!("{tx_tone:<9}"), tx_tone_style));

    spans.push(Span::styled(" Rx:", style));

    let rx_tone_style = if editing_rx_tone {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        style
    };
    spans.push(Span::styled(format!("{rx_tone:<9}"), rx_tone_style));

    spans.push(Span::styled(" ", style));
    spans.extend(duplex_spans);

    Line::from(spans)
}

fn format_frequency(hz: u64) -> String {
    let mhz = hz / 1_000_000;
    let khz = (hz % 1_000_000) / 1_000;
    let h = hz % 1_000;
    format!("{mhz:>3}.{khz:03}.{h:03}")
}

fn mode_width(mode: &crate::mode::OperatingMode) -> &'static str {
    use crate::mode::OperatingMode::*;
    match mode {
        Fm | Am | Dv => "25kHz",
        FmN | AmN => "12.5k",
    }
}

fn power_level_color(level: PowerLevel) -> Color {
    match level {
        PowerLevel::SLow => Color::Cyan,
        PowerLevel::Low1 => Color::Blue,
        PowerLevel::Low2 => Color::Green,
        PowerLevel::Mid => Color::Yellow,
        PowerLevel::High => Color::Red,
    }
}

/// Derive Tx tone display string from tone_mode and associated data.
fn tx_tone_display(state: &VfoState) -> String {
    let mode = match state.tone_mode {
        Some(m) => m,
        None => return "---".to_string(),
    };
    match mode {
        0x00 => "CSQ".to_string(),
        0x01 | 0x09 => {
            // TPL on Tx
            match state.tx_tone_freq {
                Some(f) => format!("TPL {:>5}", format_tone_freq(f)),
                None => "TPL   ---".to_string(),
            }
        }
        0x06 | 0x07 | 0x08 => {
            // DPL on Tx
            let pol = match state.dtcs_tx_pol {
                Some(0) => "+",
                Some(_) => "-",
                None => "?",
            };
            match state.dtcs_code {
                Some(c) => format!("DPL {pol}{c:03}"),
                None => format!("DPL {pol}---"),
            }
        }
        0x02 | 0x03 | 0x04 | 0x05 => "CSQ".to_string(),
        _ => "---".to_string(),
    }
}

/// Derive Rx tone display string from tone_mode and associated data.
fn rx_tone_display(state: &VfoState) -> String {
    let mode = match state.tone_mode {
        Some(m) => m,
        None => return "---".to_string(),
    };
    match mode {
        0x00 | 0x01 | 0x06 => "CSQ".to_string(),
        0x02 | 0x04 | 0x08 | 0x09 => {
            // TPL on Rx
            match state.rx_tone_freq {
                Some(f) => format!("TPL {:>5}", format_tone_freq(f)),
                None => "TPL   ---".to_string(),
            }
        }
        0x03 | 0x05 | 0x07 => {
            // DPL on Rx
            let pol = match state.dtcs_rx_pol {
                Some(0) => "+",
                Some(_) => "-",
                None => "?",
            };
            match state.dtcs_code {
                Some(c) => format!("DPL {pol}{c:03}"),
                None => format!("DPL {pol}---"),
            }
        }
        _ => "---".to_string(),
    }
}

/// Format tone frequency from tenths of Hz (e.g. 1413 → "141.3").
fn format_tone_freq(tenths: u16) -> String {
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// Display string for tone editing (shown in VFO row while editing).
fn tone_edit_display(app: &App) -> String {
    match app.tone_edit_phase {
        ToneEditPhase::SelectType => format!("{}", app.tone_type_edit),
        ToneEditPhase::SelectValue => match app.tone_type_edit {
            ToneType::Csq => "CSQ".to_string(),
            ToneType::Tpl => {
                let freq = CTCSS_TONES[app.tone_freq_edit];
                format!("TPL {:>5}", format_tone_freq(freq))
            }
            ToneType::Dpl => {
                let code = DTCS_CODES[app.dtcs_code_edit];
                let pol = if app.dtcs_pol_edit { "-" } else { "+" };
                format!("DPL {pol}{code:03}")
            }
        },
    }
}

/// Format duplex direction and offset as colored spans.
///
/// Simplex → plain "Simplex".
/// DUP+   → yellow "+ " followed by right-aligned offset in Hz with digit grouping.
/// DUP-   → cyan  "- " followed by right-aligned offset in Hz with digit grouping.
///
/// Offset format: `+  5 000 000` (10 chars for the number, space-grouped).
fn duplex_spans(state: &VfoState, base_style: Style) -> Vec<Span<'static>> {
    match state.duplex {
        Some(0x10) => vec![Span::styled("Simplex", base_style)],
        Some(dir @ (0x11 | 0x12)) => {
            let (sign, color) = if dir == 0x12 {
                ("+", Color::Yellow)
            } else {
                ("-", Color::Cyan)
            };
            let offset_str = state
                .offset
                .map(|f| format_offset_hz(f.hz()))
                .unwrap_or_else(|| "        ---".to_string());
            let style = base_style.fg(color);
            vec![
                Span::styled(format!("{sign} "), style),
                Span::styled(offset_str, style),
            ]
        }
        _ => vec![Span::styled("---", base_style)],
    }
}

/// Format an offset in Hz with space-separated digit groups, right-aligned to 10 chars.
///
/// Examples:
///   600_000   → "    600 000"
///   5_000_000 → "  5 000 000"
fn format_offset_hz(hz: u64) -> String {
    // Format the number with space-separated groups of 3 digits.
    let num_str = hz.to_string();
    let len = num_str.len();
    let mut grouped = String::new();
    for (i, ch) in num_str.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            grouped.push(' ');
        }
        grouped.push(ch);
    }
    // Right-align to 11 chars (enough for "99 999 999" with spaces).
    format!("{grouped:>11}")
}

fn render_error_log(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.error_log.is_empty() || area.height == 0 {
        return;
    }

    let visible = area.height as usize;
    let start = app.error_log.len().saturating_sub(visible);
    let lines: Vec<Line<'static>> = app.error_log[start..]
        .iter()
        .map(|(timestamp, msg)| {
            let elapsed = timestamp.elapsed().as_secs();
            let mins = elapsed / 60;
            let secs = elapsed % 60;
            Line::from(Span::styled(
                format!("  [{mins:>3}:{secs:02}] {msg}"),
                Style::default().fg(Color::Red),
            ))
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_help(app: &App) -> Line<'static> {
    let help_text: String = match app.input_mode {
        InputMode::Normal => {
            "  [Q]uit  [F]req  [M]ode  [V]FO  [A]F/Vol  [S]ql  [P]wr  [T]x Tone  [R]x Tone  +/- Vol  [0] Mute".to_string()
        }
        InputMode::Editing(Focus::Frequency) => {
            "  \u{2190}\u{2192} move cursor  \u{2191}\u{2193} change digit  0-9 type digit  Enter confirm  Esc cancel".to_string()
        }
        InputMode::Editing(Focus::Mode) => {
            "  \u{2190}\u{2192} cycle mode  Enter confirm  Esc cancel".to_string()
        }
        InputMode::Editing(Focus::AfLevel) | InputMode::Editing(Focus::Squelch) => {
            "  \u{2191}\u{2193} adjust level  Enter confirm  Esc cancel".to_string()
        }
        InputMode::Editing(Focus::Power) => {
            format!("  \u{2190}\u{2192} [{}]  Enter confirm  Esc cancel", app.power_edit.label())
        }
        InputMode::Editing(Focus::TxTone) | InputMode::Editing(Focus::RxTone) => {
            match app.tone_edit_phase {
                ToneEditPhase::SelectType => {
                    format!("  \u{2190}\u{2192} [{}]  Enter select  Esc cancel", app.tone_type_edit)
                }
                ToneEditPhase::SelectValue => match app.tone_type_edit {
                    ToneType::Tpl => {
                        let freq = CTCSS_TONES[app.tone_freq_edit];
                        format!(
                            "  \u{2191}\u{2193} tone [{}.{}]  Enter confirm  Esc back",
                            freq / 10, freq % 10
                        )
                    }
                    ToneType::Dpl => {
                        let code = DTCS_CODES[app.dtcs_code_edit];
                        let pol = if app.dtcs_pol_edit { "-" } else { "+" };
                        format!(
                            "  \u{2191}\u{2193} code  \u{2190}\u{2192} polarity [{pol}{code:03}]  Enter confirm  Esc back"
                        )
                    }
                    ToneType::Csq => "  Enter confirm  Esc cancel".to_string(),
                },
            }
        }
    };

    Line::from(Span::styled(
        help_text.to_string(),
        Style::default().fg(Color::Magenta),
    ))
}

fn render_stats(app: &App) -> Line<'static> {
    let baud = app.baud_rate;
    let tx = app.radio_state.tx_bits_per_sec;
    let rx = app.radio_state.rx_bits_per_sec;
    let total = tx + rx;
    let total_pct = if baud > 0 { total * 100 / baud } else { 0 };
    let tx_pct = if baud > 0 { tx * 100 / baud } else { 0 };
    let rx_pct = if baud > 0 { rx * 100 / baud } else { 0 };

    Line::from(vec![
        Span::raw(format!("Baud {baud} ({total_pct:>3}%)  ")),
        Span::styled(
            format!("Tx: {tx:>5} bits ({tx_pct:>2}%)"),
            Style::default().fg(Color::Red),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Rx: {rx:>5} bits ({rx_pct:>2}%)"),
            Style::default().fg(Color::Green),
        ),
    ])
}
