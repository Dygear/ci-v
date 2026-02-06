use std::io;
use std::panic;
use std::sync::mpsc as std_mpsc;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc as tokio_mpsc;

use ci_v::Radio;
use ci_v::tui::app::App;
use ci_v::tui::event::{AppEvent, EventHandler};
use ci_v::tui::message::RadioEvent;
use ci_v::tui::radio_task;
use ci_v::tui::ui;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    // Connect to radio in plain sync context (before tokio runtime starts).
    println!("CI-V Controller for ICOM ID-52A Plus");
    println!("=====================================");
    println!("Connecting to radio...");

    let radio = match Radio::auto_connect() {
        Ok(r) => {
            println!("Connected.");
            r
        }
        Err(e) => {
            eprintln!("Failed to connect: {e}");
            eprintln!();
            eprintln!("Troubleshooting:");
            eprintln!("  1. Connect the ID-52A Plus via USB-C");
            eprintln!("  2. Ensure the Following Settings on the Radio:");
            eprintln!("     Menu > Set > Function");
            eprintln!("         CI-V > CI-V Address = B4");
            eprintln!("         CI-V > CI-V Buad Rate (SP Jack) = Auto");
            eprintln!("         CI-V > CI-V Transceive = ON");
            eprintln!("         CI-V > CI-V USB/Bluetooth->Remote Transceive Address = 00");
            eprintln!("         USB Connect = Serialport");
            eprintln!("         USB Serialport Function = CI-V (Echo Back ON)");
            eprintln!("  3. Ensure the ICOM USB driver is installed");
            std::process::exit(1);
        }
    };

    let baud_rate = radio.baud_rate();

    // Start tokio runtime for the TUI.
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(async {
        // Create channels.
        // TUI → Radio: std::sync::mpsc (radio thread is blocking).
        let (cmd_tx, cmd_rx) = std_mpsc::channel();
        // Radio → TUI: tokio unbounded (async-compatible).
        let (radio_event_tx, radio_event_rx) = tokio_mpsc::unbounded_channel::<RadioEvent>();

        // Spawn blocking radio task.
        tokio::task::spawn_blocking(move || {
            radio_task::radio_loop(radio, cmd_rx, radio_event_tx);
        });

        // Run the TUI.
        if let Err(e) = run_tui(cmd_tx, radio_event_rx, baud_rate).await {
            eprintln!("TUI error: {e}");
            std::process::exit(1);
        }
    });
}

async fn run_tui(
    cmd_tx: std_mpsc::Sender<ci_v::tui::message::RadioCommand>,
    radio_event_rx: tokio_mpsc::UnboundedReceiver<RadioEvent>,
    baud_rate: u32,
) -> io::Result<()> {
    // Setup terminal.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Install panic hook to restore terminal on panic.
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));

    let mut app = App::new(cmd_tx, baud_rate);
    let mut events = EventHandler::new(radio_event_rx);

    // Main event loop.
    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if let Some(event) = events.next().await {
            match event {
                AppEvent::Key(key) => {
                    // crossterm 0.28 sends both Press and Release on some platforms.
                    if key.kind == crossterm::event::KeyEventKind::Press {
                        app.handle_key(key);
                    }
                }
                AppEvent::Radio(radio_event) => {
                    app.handle_radio_event(radio_event);
                }
                AppEvent::Tick => {
                    // Tick just triggers a redraw (handled by the loop).
                }
                AppEvent::Resize(_, _) => {
                    // Terminal auto-resizes on next draw.
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal.
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
