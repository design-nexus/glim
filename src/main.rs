mod app;
#[cfg(test)]
mod audit;
mod aurora;
mod commands;
mod config;
mod gpu;
mod lms;
mod navigation;
mod ollama;
mod proc;
mod theme;
mod ui;
mod util;

use std::io::stdout;
use std::time::Duration;

use anyhow::Context;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc::unbounded_channel;

use crate::app::{App, Bus};

struct Suspended;

impl Suspended {
    fn enter() -> Self {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
        Self
    }
}

impl Drop for Suspended {
    fn drop(&mut self) {
        let _ = enable_raw_mode();
        let _ = execute!(stdout(), EnterAlternateScreen);
        if !std::env::args().any(|a| a == "--no-mouse") {
            let _ = execute!(stdout(), EnableMouseCapture);
        }
    }
}

struct TtyGuard;

impl Drop for TtyGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("glim {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        println!("Glim — Aurora terminal interface\n\nUsage: glim [--help] [--version] [--no-mouse]\nF2 Navigate  F6 panes  Ctrl+K actions  Ctrl+C quit");
        return Ok(());
    }
    let found = theme::find_omarchy();
    let config_path = config::config_path();
    let cfg = config::Config::load(&config_path);
    let (omarchy, watch) = match found {
        Some((_, theme, path)) => (Some(theme), Some(path)),
        None => (None, None),
    };
    let theme = theme::resolve(&cfg.theme, omarchy.as_ref());
    let (tx, mut rx) = unbounded_channel();
    if let Some(path) = watch {
        let theme_tx = tx.clone();
        theme::spawn_watcher(path, move || {
            let _ = theme_tx.send(Bus::Theme);
        });
    }
    let mut app = App::new(cfg.clone(), theme, tx.clone());
    app::spawn_poller(cfg, tx);

    enable_raw_mode().context("failed to put the terminal in raw mode")?;
    let _guard = TtyGuard;
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
        previous(info);
    }));
    execute!(stdout(), EnterAlternateScreen)?;
    if !std::env::args().any(|a| a == "--no-mouse") {
        execute!(stdout(), EnableMouseCapture)?;
    }
    execute!(stdout()).context("failed to enter the alternate screen")?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut reader = EventStream::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(50));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let status = loop {
        match terminal.draw(|frame| ui::draw(frame, &mut app)) {
            Ok(rendered) => aurora::paint_wordmark(rendered.buffer)?,
            Err(err) => break Err(err).context("drawing the screen"),
        }
        if let Some(action) = app.take_service_action() {
            let suspended = Suspended::enter();
            println!();
            let result = ollama::control(action);
            drop(suspended);
            if let Err(err) = terminal.clear() {
                break Err(err).context("clearing the screen");
            }
            app.finish_service(result);
            continue;
        }
        tokio::select! {
            biased;
            event = reader.next() => {
                match event {
                    Some(Ok(Event::Key(key))) => {
                        if app.on_key(key) {
                            break Ok(());
                        }
                    }
                    Some(Ok(Event::Mouse(mouse))) => app.on_mouse(mouse),
                    Some(Ok(_)) => {}
                    Some(Err(err)) => break Err(err).context("reading terminal input"),
                    None => break Ok(()),
                }
            }
            _ = ticker.tick() => {
                app.tick();
            }
            event = rx.recv() => {
                match event {
                    Some(bus) => app.on_bus(bus),
                    None => break Ok(()),
                }
            }
        }
    };
    drop(terminal);
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
    status
}
