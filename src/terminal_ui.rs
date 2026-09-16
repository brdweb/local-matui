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
///
/// `tick` reports whether it changed anything on screen, so a still interface
/// costs no redraw at all: rebuilding every list row many times a second is not
/// free even though Ratatui only writes the cells that differ.
pub fn run(
    mut app: App,
    mut tick: impl FnMut(&mut App) -> bool,
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
    let mut dirty = true;
    let start = std::time::Instant::now();
    // The cover last written outside the cell grid, by region and generation.
    let mut drawn_art: Option<(ratatui::layout::Rect, u64)> = None;
    while !quit.load(std::sync::atomic::Ordering::Acquire) {
        dirty |= tick(&mut app);
        if app.exit {
            break;
        }
        // Snapshots are seconds apart; the position on screen moves between
        // them, but only a whole second changes anything a viewer can see.
        let second = app.elapsed as u64;
        app.advance(std::time::Instant::now());
        dirty |= app.elapsed as u64 != second;
        if theme_check.elapsed() >= Duration::from_millis(500) {
            let previous = app.palette;
            crate::theme::reload(&mut app.palette, &theme_paths);
            dirty |= app.palette != previous;
            theme_check = std::time::Instant::now();
        }
        // A title too wide for its column scrolls, one column per step. The
        // step comes from here rather than from the clock inside `draw`, so a
        // still title costs nothing and drawing stays a function of state.
        let step = start.elapsed().as_millis() as u64 / 140;
        if step != app.tick {
            app.tick = step;
            dirty |= app.scrolling;
        }
        // The spectrum animates from the audio rather than from state changes,
        // so it earns frames of its own at about 60 per second. Everything else
        // still costs nothing when nothing has changed. `app.animating` is set
        // by the previous draw, so the first frame that shows bars comes from
        // `dirty` and every frame after it from this.
        let animating = app.animating;
        if dirty || animating {
            app.animating = false;
            terminal.draw(|frame| ui::draw(frame, &mut app))?;
            dirty = false;
            // Sixel writes pixels the cell renderer knows nothing about, so it
            // goes out after the cells are flushed and only when what it drew
            // has actually gone stale. The region itself is left blank, so a
            // later diff has nothing to paint back over the image.
            let stamp = app.artwork_area.map(|area| (area, app.artwork_generation));
            if stamp != drawn_art {
                // Pixels already on screen outlive the cells they sit in: the
                // renderer only rewrites cells whose contents changed, so a
                // view drawn over the cover — a menu, say — leaves whatever it
                // did not happen to write text into. Repainting everything is
                // the only way to take those pixels back, and it is affordable
                // because it happens when the cover appears, moves or goes,
                // not on the frames in between.
                if drawn_art.is_some() {
                    terminal.clear()?;
                    terminal.draw(|frame| ui::draw(frame, &mut app))?;
                }
                if let (Some((area, _)), Some(art)) = (stamp, app.artwork.as_ref()) {
                    if let Some((cell_width, cell_height)) = crate::artwork::cell_pixels() {
                        execute!(io::stdout(), cursor::MoveTo(area.x, area.y))?;
                        print!(
                            "{}",
                            art.sixel(area.width * cell_width, area.height * cell_height)
                        );
                        io::Write::flush(&mut io::stdout())?;
                    }
                }
                drawn_art = stamp;
            }
        }
        let interval = if animating {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(50)
        };
        if event::poll(interval)? {
            let action = match event::read()? {
                event::Event::Key(key) => {
                    dirty = true;
                    app.key(key)
                }
                event::Event::Paste(text) => {
                    dirty = true;
                    app.paste(&text);
                    Action::None
                }
                // A resize invalidates the whole frame, not just what changed.
                event::Event::Resize(..) => {
                    dirty = true;
                    // A resize repaints everything, including over any pixels
                    // written outside the cell grid.
                    drawn_art = None;
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
