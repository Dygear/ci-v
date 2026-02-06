use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::app::{self, App, Focus, InputMode};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Main border.
    let status = if app.connected { "Connected" } else { "Disconnected" };
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

    // Split inner area into sections.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // blank
            Constraint::Length(1), // frequency
            Constraint::Length(1), // blank
            Constraint::Length(1), // mode + vfo
            Constraint::Length(1), // blank
            Constraint::Length(1), // s-meter
            Constraint::Length(1), // af level
            Constraint::Length(1), // squelch
            Constraint::Length(1), // blank
            Constraint::Length(1), // error line
            Constraint::Min(0),   // spacer
            Constraint::Length(1), // help bar
        ])
        .split(inner);

    // Frequency line.
    let freq_line = render_frequency(app);
    frame.render_widget(Paragraph::new(freq_line), chunks[1]);

    // Mode + VFO line.
    let mode_vfo_line = render_mode_vfo(app);
    frame.render_widget(Paragraph::new(mode_vfo_line), chunks[3]);

    // S-meter.
    let s_line = render_meter(app, "S-Meter", app.radio_state.s_meter, Color::Green, false);
    frame.render_widget(Paragraph::new(s_line), chunks[5]);

    // Volume.
    let is_editing_vol = app.input_mode == InputMode::Editing(Focus::AfLevel);
    let vol_line = render_volume(app, is_editing_vol);
    frame.render_widget(Paragraph::new(vol_line), chunks[6]);

    // Squelch.
    let is_editing_sql = app.input_mode == InputMode::Editing(Focus::Squelch);
    let sql_val = if is_editing_sql {
        Some(app.sql_edit)
    } else {
        app.radio_state.squelch
    };
    let sql_line = render_meter(app, "Squelch ", sql_val, Color::Yellow, is_editing_sql);
    frame.render_widget(Paragraph::new(sql_line), chunks[7]);

    // Error line.
    if let Some(ref err) = app.last_error {
        let err_line = Line::from(Span::styled(
            format!("  Error: {err}"),
            Style::default().fg(Color::Red),
        ));
        frame.render_widget(Paragraph::new(err_line), chunks[9]);
    }

    // Help bar: left-aligned help text + right-aligned stats.
    let help_area = chunks[11];
    let help_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(62)])
        .split(help_area);

    let help = render_help(app);
    frame.render_widget(Paragraph::new(help), help_chunks[0]);

    let stats = render_stats(app);
    frame.render_widget(Paragraph::new(stats), help_chunks[1]);
}

fn render_frequency(app: &App) -> Line<'static> {
    let is_editing = app.input_mode == InputMode::Editing(Focus::Frequency);
    let hz = if is_editing {
        app.freq_edit_hz
    } else {
        app.radio_state.frequency.map(|f| f.hz()).unwrap_or(0)
    };

    let digits = app.freq_digits(hz);
    let mut spans = vec![Span::styled("  Frequency:  ", Style::default().fg(Color::White))];

    for (i, &d) in digits.iter().enumerate() {
        // Insert dots before positions 3 and 6.
        if i == 3 || i == 6 {
            spans.push(Span::styled(".", Style::default().fg(Color::DarkGray)));
        }

        let ch = format!("{d}");
        let style = if is_editing && i == app.freq_cursor {
            // Cursor digit: reverse video.
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else if is_editing {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        };
        spans.push(Span::styled(ch, style));
    }

    spans.push(Span::styled(" MHz", Style::default().fg(Color::DarkGray)));

    if !is_editing && app.radio_state.frequency.is_none() {
        return Line::from(vec![
            Span::styled("  Frequency:  ", Style::default().fg(Color::White)),
            Span::styled("---.--.--- MHz", Style::default().fg(Color::DarkGray)),
        ]);
    }

    Line::from(spans)
}

fn render_mode_vfo(app: &App) -> Line<'static> {
    let is_editing_mode = app.input_mode == InputMode::Editing(Focus::Mode);
    let is_editing_vfo = app.input_mode == InputMode::Editing(Focus::Vfo);

    let mode_str = if is_editing_mode {
        format!("{}", app.mode_edit)
    } else {
        app.radio_state
            .mode
            .map(|m| format!("{m}"))
            .unwrap_or_else(|| "---".to_string())
    };

    let mode_style = if is_editing_mode {
        Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };

    let vfo_str = if is_editing_vfo {
        format!("{}", app.vfo_edit)
    } else {
        "A".to_string()
    };

    let vfo_style = if is_editing_vfo {
        Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };

    Line::from(vec![
        Span::styled("  Mode:       ", Style::default().fg(Color::White)),
        Span::styled(format!("{mode_str:<5}"), mode_style),
        Span::styled("       VFO: ", Style::default().fg(Color::White)),
        Span::styled(vfo_str, vfo_style),
    ])
}

fn render_meter(
    _app: &App,
    label: &str,
    value: Option<u16>,
    color: Color,
    is_editing: bool,
) -> Line<'static> {
    let (val, pct, raw) = match value {
        Some(v) => {
            let p = (v as f64 / 255.0 * 100.0) as u16;
            (v, p, format!("{v:>3}/255"))
        }
        None => (0, 0, "---/255".to_string()),
    };

    let bar_width = 20;
    let filled = (val as usize * bar_width / 255).min(bar_width);
    let empty = bar_width - filled;

    let bar_filled: String = "\u{2588}".repeat(filled);
    let bar_empty: String = "\u{2591}".repeat(empty);

    let label_style = if is_editing {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    Line::from(vec![
        Span::styled(format!("  {label}: "), label_style),
        Span::styled(bar_filled, Style::default().fg(color)),
        Span::styled(bar_empty, Style::default().fg(Color::DarkGray)),
        Span::styled(format!("  {pct:>3}%  {raw}"), Style::default().fg(Color::White)),
    ])
}

fn render_volume(app: &App, is_editing: bool) -> Line<'static> {
    let step = if is_editing {
        app.af_edit
    } else {
        app.radio_state
            .af_level
            .map(app::raw_to_volume_step)
            .unwrap_or(0)
    };

    let bar_width = 20;
    let filled = (step as usize * bar_width / 39).min(bar_width);
    let empty = bar_width - filled;

    let bar_filled: String = "\u{2588}".repeat(filled);
    let bar_empty: String = "\u{2591}".repeat(empty);

    let label_style = if is_editing {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let step_display = if !is_editing && app.radio_state.af_level.is_none() {
        "--/39".to_string()
    } else {
        format!("{step:>2}/39")
    };

    let mut spans = vec![
        Span::styled("  Volume:  ", label_style),
        Span::styled(bar_filled, Style::default().fg(Color::Cyan)),
        Span::styled(bar_empty, Style::default().fg(Color::DarkGray)),
        Span::styled(format!("  {step_display}"), Style::default().fg(Color::White)),
    ];

    if app.mute_restore_step.is_some() {
        spans.push(Span::styled("  MUTE", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)));
    }

    Line::from(spans)
}

fn render_help(app: &App) -> Line<'static> {
    let help_text = match app.input_mode {
        InputMode::Normal => {
            "  [Q]uit  [F]req  [M]ode  [V]FO  [A]F/Vol  [S]ql  +/- Vol  [0] Mute  Arrows: Freq"
        }
        InputMode::Editing(Focus::Frequency) => {
            "  \u{2190}\u{2192} move cursor  \u{2191}\u{2193} change digit  0-9 type digit  Enter confirm  Esc cancel"
        }
        InputMode::Editing(Focus::Mode) => {
            "  \u{2190}\u{2192} cycle mode  Enter confirm  Esc cancel"
        }
        InputMode::Editing(Focus::AfLevel) | InputMode::Editing(Focus::Squelch) => {
            "  \u{2191}\u{2193} adjust level  Enter confirm  Esc cancel"
        }
        InputMode::Editing(Focus::Vfo) => {
            "  \u{2190}\u{2192} toggle A/B  Enter confirm  Esc cancel"
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
        Span::styled(format!("Tx: {tx:>5} bits ({tx_pct:>2}%)"), Style::default().fg(Color::Red)),
        Span::raw("  "),
        Span::styled(format!("Rx: {rx:>5} bits ({rx_pct:>2}%)"), Style::default().fg(Color::Green)),
    ])
}
