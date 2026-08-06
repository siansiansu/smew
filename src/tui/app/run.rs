//! The TUI entry point: terminal setup, the input and tick threads, and the
//! draw/handle message loop.

use std::time::Duration;

use crossterm::event::{Event, KeyEventKind};

use super::{Model, Msg, Options};
use crate::tui::view;

/// Runs the TUI: terminal setup, input/tick threads, and the message loop.
pub fn run(opts: Options) -> std::io::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel::<Msg>();
    let mut model = Model::new(opts, tx.clone());
    model.init();

    let mut terminal = ratatui::init();
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableBracketedPaste);
    if model.mouse {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture);
    }
    if let Ok(size) = terminal.size() {
        model.width = size.width;
        model.height = size.height;
    }

    // Input thread: crossterm events → messages.
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            loop {
                let msg = match crossterm::event::read() {
                    Ok(Event::Key(k)) if k.kind != KeyEventKind::Release => Msg::Key(k),
                    Ok(Event::Mouse(me)) => Msg::Mouse(me),
                    Ok(Event::Resize(w, h)) => Msg::Resize(w, h),
                    Ok(Event::Paste(s)) => Msg::Paste(s),
                    Ok(_) => continue,
                    Err(_) => return,
                };
                if tx.send(msg).is_err() {
                    return;
                }
            }
        });
    }

    // Auto-refresh tick thread (Duration::ZERO = off).
    if model.refresh > Duration::ZERO {
        let tx = tx.clone();
        let every = model.refresh;
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(every);
                if tx.send(Msg::Tick).is_err() {
                    return;
                }
            }
        });
    }

    let res = loop {
        model.ensure_cursor_visible();
        if let Err(e) = terminal.draw(|f| view::draw(&model, f)) {
            break Err(e);
        }
        let Ok(msg) = rx.recv() else { break Ok(()) };
        model.handle(msg);
        // Drain whatever else is queued before re-rendering (bounded so a
        // flood of pane output can't starve the renderer).
        for _ in 0..128 {
            match rx.try_recv() {
                Ok(m) => model.handle(m),
                Err(_) => break,
            }
        }
        if model.should_quit() {
            break Ok(());
        }
    };

    // End any live SSM sessions before leaving the alt screen.
    for p in &model.panes {
        p.close();
    }
    if model.mouse {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    }
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableBracketedPaste);
    ratatui::restore();
    res
}
