use crate::ui::{self, Action, App};
use anyhow::{bail, Result};
use crossterm::{cursor, event, execute, terminal};
use std::{
    io::{self, IsTerminal},
    time::Duration,
};

struct Restore;
struct SignalTask(tokio::task::JoinHandle<()>);
impl Drop for SignalTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            event::DisableBracketedPaste,
            terminal::LeaveAlternateScreen,
            cursor::Show
        );
    }
}

/// Blocking rendering loop. Network/audio run on separate runtime workers.
pub fn run(
    mut app: App,
    mut tick: impl FnMut(&mut App),
    mut dispatch: impl FnMut(&mut App, Action),
) -> Result<Action> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("An interactive terminal is required; use --demo --snapshot for plain output");
    }
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let quit = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal_quit = quit.clone();
    let _signals = SignalTask(tokio::spawn(async move {
        tokio::select! { _=terminate.recv()=>{}, _=interrupt.recv()=>{} }
        signal_quit.store(true, std::sync::atomic::Ordering::Release);
    }));
    static PANIC_HOOK: std::sync::Once = std::sync::Once::new();
    PANIC_HOOK.call_once(|| {
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(
                io::stdout(),
                event::DisableBracketedPaste,
                terminal::LeaveAlternateScreen,
                cursor::Show
            );
            original_hook(info);
        }));
    });
    terminal::enable_raw_mode()?;
    let _restore = Restore;
    execute!(
        io::stdout(),
        terminal::EnterAlternateScreen,
        event::EnableBracketedPaste,
        cursor::Hide
    )?;
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    let theme_paths = crate::theme::paths();
    crate::theme::reload(&mut app.palette, &theme_paths);
    let mut theme_check = std::time::Instant::now();
    let mut outcome = Action::Quit;
    while !quit.load(std::sync::atomic::Ordering::Acquire) {
        tick(&mut app);
        if app.exit {
            break;
        }
        if theme_check.elapsed() >= Duration::from_millis(500) {
            crate::theme::reload(&mut app.palette, &theme_paths);
            theme_check = std::time::Instant::now();
        }
        terminal.draw(|frame| ui::draw(frame, &mut app))?;
        // The visualizer is the only view that animates; it is worth redrawing
        // at about 60 per second while it is open, and no more otherwise.
        let interval = if app.visualizer.mode == crate::visualizer::Mode::Off {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(16)
        };
        if event::poll(interval)? {
            let action = match event::read()? {
                event::Event::Key(key) => app.key(key),
                event::Event::Paste(text) => {
                    app.paste(&text);
                    Action::None
                }
                _ => Action::None,
            };
            if matches!(action, Action::Quit | Action::OpenSettings) {
                outcome = action;
                break;
            }
            if action != Action::None {
                dispatch(&mut app, action);
            }
        }
    }
    Ok(outcome)
}
