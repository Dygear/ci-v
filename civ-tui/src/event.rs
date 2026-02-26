use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEvent};
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::message::RadioEvent;

/// Unified application event.
#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Radio(RadioEvent),
    Tick,
    Resize(u16, u16),
}

/// Merges terminal events, radio events, and a tick timer into a single stream.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl EventHandler {
    pub fn new(radio_rx: mpsc::UnboundedReceiver<RadioEvent>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Terminal events task.
        let tx_term = tx.clone();
        tokio::spawn(async move {
            let mut reader = EventStream::new();
            while let Some(Ok(event)) = reader.next().await {
                let app_event = match event {
                    Event::Key(key) => AppEvent::Key(key),
                    Event::Resize(w, h) => AppEvent::Resize(w, h),
                    _ => continue,
                };
                if tx_term.send(app_event).is_err() {
                    break;
                }
            }
        });

        // Radio events forwarding task.
        let tx_radio = tx.clone();
        tokio::spawn(async move {
            let mut radio_rx = radio_rx;
            while let Some(event) = radio_rx.recv().await {
                if tx_radio.send(AppEvent::Radio(event)).is_err() {
                    break;
                }
            }
        });

        // Tick timer task (~20 FPS).
        let tx_tick = tx;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(50));
            loop {
                interval.tick().await;
                if tx_tick.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        });

        Self { rx }
    }

    /// Wait for the next event.
    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }
}
