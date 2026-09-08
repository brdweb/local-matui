use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::theme::Palette;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    None,
    OpenSettings,
    SaveSettings,
    Command(crate::controls::Command),
    Prompt(crate::controls::Prompt),
    Quit,
    Refresh,
    Select(String),
    Search(String),
    Browse {
        generation: u64,
        target: crate::music::Target,
    },
    Toggle,
    Next,
    Previous,
    /// Absolute position in seconds, resolved by the interface.
    Seek(f64),
    Play(String),
    Enqueue(String),
    PlayNext(String),
}

impl App {
    /// A queue mode change for the displayed queue. Dynamic queues report no
    /// shuffle or repeat state, so the key says so rather than guessing.
    fn queue_mode(&mut self, name: &'static str, args: serde_json::Value) -> Action {
        if self.queue_id.is_empty() {
            self.status = "No active queue for this player yet".into();
            return Action::None;
        }
        if self.queue_details["is_dynamic"] == true {
            self.status = "A dynamic queue has no shuffle or repeat setting".into();
            return Action::None;
        }
        Action::Command(crate::controls::Command::Queue {
            id: self.queue_id.clone(),
            name,
            args,
        })
    }

    /// Seek relative to the position already on screen, so repeated presses
    /// accumulate without a queue request each time. The next poll corrects it.
    fn seek(&mut self, delta: f64) -> Action {
        if !self.duration.is_finite() || self.duration <= 0.0 || !self.elapsed.is_finite() {
            self.status = "This item has no seekable duration".into();
            return Action::None;
        }
        self.elapsed = (self.elapsed + delta).clamp(0.0, self.duration);
        Action::Seek(self.elapsed)
    }

    /// Why the visualizer has no samples, in terms of the selected speaker.
    /// A remote speaker's audio never reaches this machine.
    fn silence(&self) -> String {
        let selected = self
            .players
            .iter()
            .find(|p| Some(&p.id) == self.selected_id.as_ref());
        let local = selected
            .zip(self.local_endpoint.as_deref())
            .is_some_and(|(p, endpoint)| crate::presentation::matches_endpoint(p, endpoint));
        match selected {
            Some(player) if local => format!(
                "no local audio · this speaker is {}",
                if player.state.is_empty() {
                    "idle"
                } else {
                    player.state.as_str()
                }
            ),
            Some(player) => format!("no local audio · playing on {}", player.name),
            None => "no local audio · select Local Matui's own speaker".into(),
        }
    }

    /// Pasted text never becomes shortcuts, field navigation or submission.
    pub fn paste(&mut self, text: &str) {
        if let Some(settings) = &mut self.settings {
            settings.paste(text);
        } else if let Some(menu) = &mut self.menu {
            if let Some(prompt) = &mut menu.prompt {
                if !append_paste(&mut prompt.value, text, 2048) {
                    menu.error = "Paste exceeds this field's 2048-byte limit".into();
                }
            }
        } else if self.editing && !append_paste(&mut self.query, text, 256) {
            self.status = "Paste exceeds the search field's 256-byte limit".into();
        }
    }

    pub fn key(&mut self, key: crossterm::event::KeyEvent) -> Action {
        use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
        if key.kind == KeyEventKind::Release {
            return Action::None;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Action::Quit;
        }
        if let Some(settings) = &mut self.settings {
            return settings.key(key);
        }
        if self.menu.is_some() {
            return crate::controls::key(self, key);
        }
        if self.editing {
            match key.code {
                KeyCode::Esc => self.editing = false,
                KeyCode::Enter => {
                    self.editing = false;
                    self.search_cursor = 0;
                    if !self.query.trim().is_empty() {
                        return Action::Search(self.query.trim().into());
                    }
                }
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Char(c) if !c.is_control() && self.query.len() < 256 => self.query.push(c),
                _ => {}
            }
            return Action::None;
        }
        if self.focus == Focus::Music {
            if let Some(action) = crate::music::key(self, key) {
                return action;
            }
        }
        if self.focus == Focus::Search
            && matches!(
                key.code,
                KeyCode::Enter | KeyCode::Char('P') | KeyCode::Char('a') | KeyCode::Char('N')
            )
        {
            if let Some(media) = self
                .results
                .get(self.search_cursor)
                .and_then(|t| t.media.clone())
            {
                if key.code == KeyCode::Enter {
                    if let Some(target) = media.open.clone() {
                        self.focus = Focus::Music;
                        self.content = Focus::Music;
                        return self.music.navigate(target, media.title);
                    }
                }
                if matches!(key.code, KeyCode::Enter | KeyCode::Char('P')) {
                    return crate::music::choose(self, &media);
                }
                crate::music::choose(self, &media);
                if self.menu.take().is_some() {
                    return if key.code == KeyCode::Char('a') {
                        Action::Enqueue(media.uri)
                    } else {
                        Action::PlayNext(media.uri)
                    };
                }
                return Action::None;
            }
        }
        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::F(3) | KeyCode::Char('b') => {
                self.focus = Focus::Music;
                self.content = Focus::Music;
                Action::None
            }
            KeyCode::F(4) => {
                self.focus = Focus::Queue;
                Action::None
            }
            KeyCode::F(2) => Action::OpenSettings,
            KeyCode::Char('v') => {
                if self.spectrum.is_none() {
                    self.status =
                        "Visualizer needs Local Matui's own speaker: enable local audio (F2)"
                            .into();
                } else {
                    self.visualizer.mode = self.visualizer.mode.next();
                }
                Action::None
            }
            KeyCode::Char('?') | KeyCode::F(1) => {
                self.menu = Some(crate::controls::Menu::new(self));
                Action::None
            }
            KeyCode::Char('/') => {
                self.editing = true;
                self.focus = Focus::Search;
                self.content = Focus::Search;
                self.query.clear();
                Action::None
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if key.code == KeyCode::BackTab {
                    match self.focus {
                        Focus::Players => Focus::Search,
                        Focus::Music => Focus::Players,
                        Focus::Queue => Focus::Music,
                        Focus::Search => Focus::Queue,
                    }
                } else {
                    match self.focus {
                        Focus::Players => Focus::Music,
                        Focus::Music => Focus::Queue,
                        Focus::Queue => Focus::Search,
                        Focus::Search => Focus::Players,
                    }
                };
                // Music and search share the right pane; the queue has its own.
                if matches!(self.focus, Focus::Music | Focus::Search) {
                    self.content = self.focus;
                }
                Action::None
            }
            KeyCode::Esc if self.visualizer.mode != crate::visualizer::Mode::Off => {
                self.visualizer.mode = crate::visualizer::Mode::Off;
                Action::None
            }
            KeyCode::Esc => {
                match self.focus {
                    // Leave search results for the browser they came from.
                    Focus::Search => {
                        self.focus = Focus::Music;
                        self.content = Focus::Music;
                    }
                    Focus::Music if !self.music.history.is_empty() => self.music.back(),
                    _ => {}
                }
                Action::None
            }
            KeyCode::Down
            | KeyCode::Char('j')
            | KeyCode::Up
            | KeyCode::Char('k')
            | KeyCode::PageDown
            | KeyCode::PageUp
            | KeyCode::Home
            | KeyCode::End => {
                let down = matches!(key.code, KeyCode::Down | KeyCode::Char('j'));
                let (cursor, len) = match self.focus {
                    Focus::Players => (&mut self.player_cursor, self.players.len()),
                    Focus::Queue => (&mut self.queue_cursor, self.queue.len()),
                    Focus::Search => (&mut self.search_cursor, self.results.len()),
                    Focus::Music => (&mut self.music.page.cursor, self.music.page.items.len()),
                };
                *cursor = if key.code == KeyCode::Home {
                    0
                } else if key.code == KeyCode::End {
                    len.saturating_sub(1)
                } else if key.code == KeyCode::PageDown {
                    cursor.saturating_add(10).min(len.saturating_sub(1))
                } else if key.code == KeyCode::PageUp {
                    cursor.saturating_sub(10)
                } else if down {
                    cursor.saturating_add(1).min(len.saturating_sub(1))
                } else {
                    cursor.saturating_sub(1)
                };
                Action::None
            }
            KeyCode::Enter if self.focus == Focus::Players && self.connected => {
                if let Some(p) = self.players.get(self.player_cursor).filter(|p| p.available) {
                    self.selected_id = Some(p.id.clone());
                    self.queue.clear();
                    self.queue_id.clear();
                    self.queue_details = serde_json::Value::Null;
                    self.title = "Loading queue…".into();
                    self.artist.clear();
                    self.elapsed = 0.0;
                    self.duration = 0.0;
                    self.focus = Focus::Music;
                    self.content = Focus::Music;
                    Action::Select(p.id.clone())
                } else {
                    Action::None
                }
            }
            KeyCode::Char('r') => Action::Refresh,
            _ if !self.connected
                || !self
                    .players
                    .iter()
                    .any(|p| Some(&p.id) == self.selected_id.as_ref() && p.available) =>
            {
                Action::None
            }
            KeyCode::Char(' ') | KeyCode::Char('p') => Action::Toggle,
            KeyCode::Char('n') | KeyCode::Char('>') | KeyCode::Char('.') => Action::Next,
            KeyCode::Char('<') | KeyCode::Char(',') => Action::Previous,
            KeyCode::Char('s') => Action::Command(crate::controls::Command::Player {
                name: "stop",
                args: serde_json::json!({}),
            }),
            // z and l keep shuffle and repeat off any shifted pair.
            KeyCode::Char('z') => self.queue_mode(
                "shuffle",
                serde_json::json!({
                    "shuffle_enabled": self.queue_details["shuffle_enabled"] != true
                }),
            ),
            KeyCode::Char('l') => {
                // off → all → one → off, matching the controls menu's modes.
                let next = match self.queue_details["repeat_mode"].as_str() {
                    Some("all") => "one",
                    Some("one") => "off",
                    _ => "all",
                };
                self.queue_mode("repeat", serde_json::json!({ "repeat_mode": next }))
            }
            KeyCode::Char('m') => {
                let muted = self
                    .players
                    .iter()
                    .find(|p| Some(&p.id) == self.selected_id.as_ref())
                    .and_then(|p| p.details["volume_muted"].as_bool());
                muted
                    .map(|v| {
                        Action::Command(crate::controls::Command::Player {
                            name: "volume_mute",
                            args: serde_json::json!({"muted":!v}),
                        })
                    })
                    .unwrap_or(Action::None)
            }
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Char('-') => {
                Action::Command(crate::controls::Command::Player {
                    name: if key.code == KeyCode::Char('-') {
                        "volume_down"
                    } else {
                        "volume_up"
                    },
                    args: serde_json::json!({}),
                })
            }
            KeyCode::Left | KeyCode::Right => self.seek(if key.code == KeyCode::Left {
                -10.0
            } else {
                10.0
            }),
            KeyCode::Enter | KeyCode::Delete | KeyCode::Char('J') | KeyCode::Char('K')
                if self.focus == Focus::Queue =>
            {
                let Some(item) = self
                    .queue
                    .get(self.queue_cursor)
                    .filter(|t| !t.id.is_empty())
                else {
                    return Action::None;
                };
                let (name, args) = match key.code {
                    KeyCode::Enter => ("play_index", serde_json::json!({"index":item.id})),
                    KeyCode::Delete => (
                        "delete_item",
                        serde_json::json!({"item_id_or_index":item.id}),
                    ),
                    KeyCode::Char('J') => (
                        "move_item",
                        serde_json::json!({"queue_item_id":item.id,"pos_shift":1}),
                    ),
                    _ => (
                        "move_item",
                        serde_json::json!({"queue_item_id":item.id,"pos_shift":-1}),
                    ),
                };
                Action::Command(crate::controls::Command::Queue {
                    id: self.queue_id.clone(),
                    name,
                    args,
                })
            }
            KeyCode::Enter | KeyCode::Char('a') if self.focus == Focus::Search => {
                if let Some(t) = self.results.get(self.search_cursor) {
                    if key.code == KeyCode::Enter {
                        Action::Play(t.uri.clone())
                    } else {
                        Action::Enqueue(t.uri.clone())
                    }
                } else {
                    Action::None
                }
            }
            _ => Action::None,
        }
    }
}

/// Single-line inputs exclude terminal control characters. Reject oversized
/// pastes atomically rather than silently truncating a URL or credential.
pub(crate) fn append_paste(value: &mut String, text: &str, limit: usize) -> bool {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    if text.len() > limit.saturating_sub(value.len()) {
        return false;
    }
    value.push_str(&text);
    true
}

#[derive(Clone, Default)]
pub struct PlayerView {
    pub details: serde_json::Value,
    pub id: String,
    pub name: String,
    pub state: String,
    pub available: bool,
    pub volume: Option<u8>,
}

#[derive(Clone, Default)]
pub struct TrackView {
    pub media: Option<crate::music::Media>,
    pub id: String,
    pub uri: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Players,
    Music,
    Queue,
    Search,
}

pub struct App {
    pub music: crate::music::Browser,
    /// Decoded local samples, when Local Matui itself is a speaker this run.
    pub spectrum: Option<crate::visualizer::Analyzer>,
    pub visualizer: crate::visualizer::Meter,
    /// Persistent identity of Local Matui's own endpoint, for explaining an empty
    /// visualizer when a different speaker is selected.
    pub local_endpoint: Option<String>,
    pub content: Focus,
    pub menu: Option<crate::controls::Menu>,
    pub queue_id: String,
    pub queue_details: serde_json::Value,
    pub palette: Palette,
    pub settings: Option<crate::settings::Settings>,
    pub exit: bool,
    pub players: Vec<PlayerView>,
    pub queue: Vec<TrackView>,
    pub results: Vec<TrackView>,
    pub selected_id: Option<String>,
    pub title: String,
    pub artist: String,
    pub elapsed: f64,
    pub duration: f64,
    pub status: String,
    pub audio_status: String,
    pub focus: Focus,
    pub player_cursor: usize,
    pub queue_cursor: usize,
    pub search_cursor: usize,
    pub editing: bool,
    pub query: String,
    pub demo: bool,
    pub connected: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            music: crate::music::Browser::default(),
            spectrum: None,
            visualizer: crate::visualizer::Meter::default(),
            local_endpoint: None,
            content: Focus::Music,
            menu: None,
            queue_id: String::new(),
            queue_details: serde_json::Value::Null,
            palette: Palette::default(),
            settings: None,
            exit: false,
            players: vec![],
            queue: vec![],
            results: vec![],
            selected_id: None,
            title: "No player selected".into(),
            artist: "Select a player and press Enter".into(),
            elapsed: 0.0,
            duration: 0.0,
            status: "Disconnected".into(),
            audio_status: "Local audio disabled".into(),
            focus: Focus::Players,
            player_cursor: 0,
            queue_cursor: 0,
            search_cursor: 0,
            editing: false,
            query: String::new(),
            demo: false,
            connected: false,
        }
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let palette = app.palette;
    if let Some(settings) = &app.settings {
        settings.draw(frame, palette);
        return;
    }
    if app.menu.is_some() {
        crate::controls::draw(frame, app);
        return;
    }
    let area = frame.area();
    frame.render_widget(
        Block::default().style(
            Style::default()
                .bg(palette.background)
                .fg(palette.foreground),
        ),
        area,
    );
    if area.width < 50 || area.height < 16 {
        frame.render_widget(
            Paragraph::new("LOCAL-MATUI\nResize to 50 x 16\nq: quit")
                .style(Style::default().fg(palette.accent)),
            area,
        );
        return;
    }
    if app.visualizer.mode == crate::visualizer::Mode::Full {
        draw_visualizer(frame, app, area);
        return;
    }
    // Chrome is two header rows, four for now playing, three for status and
    // two for hints; everything else belongs to the lists.
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(4),
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .split(area);
    let mode = if app.demo {
        "OFFLINE DEMO · sample data"
    } else {
        "MUSIC ASSISTANT · LOCAL + REMOTE"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  LOCAL-MATUI  ",
                Style::default()
                    .fg(palette.background)
                    .bg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  {mode}")),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(palette.secondary)),
        ),
        rows[0],
    );
    draw_now_playing(frame, app, rows[1]);

    let cols =
        Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)]).split(rows[2]);
    // Players are few and short; the queue takes the rest of the column so it
    // stays visible while browsing.
    let listed = (app.players.len() * 2 + 2) as u16;
    let side = Layout::vertical([
        Constraint::Length(listed.clamp(4, (cols[0].height / 2).max(4))),
        Constraint::Min(3),
    ])
    .split(cols[0]);
    draw_players(frame, app, side[0]);
    draw_queue(frame, app, side[1]);
    if app.visualizer.mode == crate::visualizer::Mode::Panel {
        draw_spectrum_panel(frame, app, cols[1]);
    } else if app.content == Focus::Search || app.focus == Focus::Search || app.editing {
        draw_search(frame, app, cols[1]);
    } else {
        crate::music::draw(frame, app, cols[1]);
    }

    let message = if app.editing {
        format!("Search: {}▏  [Enter: submit · Esc: cancel]", app.query)
    } else {
        format!("{}\n{}", app.status, app.audio_status)
    };
    frame.render_widget(
        Paragraph::new(message).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(palette.secondary)),
        ),
        rows[3],
    );
    frame.render_widget(
        Paragraph::new(format!("{}\n{}", hints(app), TRANSPORT_HINTS))
            .style(Style::default().fg(palette.secondary)),
        rows[4],
    );
}

/// Keys for the focused pane. The transport line below it never changes.
fn hints(app: &App) -> &'static str {
    if app.editing {
        return "Enter submits the search · Esc cancels";
    }
    match app.focus {
        Focus::Players => "Enter select speaker · ↑↓ move · Tab pane · b music · / search · F2 settings",
        Focus::Queue => "Enter play item · Delete remove · Shift-J/K move · Tab pane · b music",
        Focus::Music => {
            "Enter open · P play collection · a add · N play next · Backspace back · ] page · r reload"
        }
        Focus::Search => "Enter play · a add · N play next · / new search · Esc back to music",
    }
}

// Kept to 102 columns so it survives a narrow terminal; everything else lives
// in the controls menu.
const TRANSPORT_HINTS: &str = "Space/p pause · </> track · s stop · +/- vol · m mute · z shuffle · l repeat · v spectrum · ? all keys";

/// Title, artist, transport state and progress, in four rows.
fn draw_now_playing(frame: &mut Frame, app: &App, area: Rect) {
    let palette = app.palette;
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(format!("  {}", app.title))
            .style(Style::default().add_modifier(Modifier::BOLD)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(format!("  {}", app.artist)).style(Style::default().fg(palette.secondary)),
        rows[1],
    );
    // The state line answers what the header used to leave to other panes:
    // whether it is playing, how loud, and how the queue is ordered.
    let player = app
        .players
        .iter()
        .find(|p| Some(&p.id) == app.selected_id.as_ref());
    let mut facts: Vec<String> = Vec::new();
    match player {
        Some(p) if !p.available => facts.push("unavailable".into()),
        Some(p) => {
            facts.push(if p.state.is_empty() {
                "idle".into()
            } else {
                p.state.clone()
            });
            if let Some(volume) = p.volume {
                facts.push(format!("vol {volume}%"));
            }
            if p.details["volume_muted"] == true {
                facts.push("muted".into());
            }
        }
        None => facts.push("no speaker selected".into()),
    }
    if !app.queue_id.is_empty() && app.queue_details["is_dynamic"] != true {
        facts.push(format!(
            "shuffle {}",
            if app.queue_details["shuffle_enabled"] == true {
                "on"
            } else {
                "off"
            }
        ));
        facts.push(format!(
            "repeat {}",
            match app.queue_details["repeat_mode"].as_str() {
                Some("all") => "all",
                Some("one") => "one",
                _ => "off",
            }
        ));
    }
    let state = Layout::horizontal([Constraint::Min(10), Constraint::Length(16)]).split(rows[2]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {} ", transport(player.map_or("", |p| p.state.as_str()))),
                Style::default().fg(palette.accent),
            ),
            Span::raw(facts.join(" · ")),
        ])),
        state[0],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{} / {}  ",
            duration(app.elapsed),
            duration(app.duration)
        ))
        .alignment(ratatui::layout::Alignment::Right),
        state[1],
    );
    frame.render_widget(
        Gauge::default()
            .ratio(progress(app))
            .gauge_style(Style::default().fg(palette.accent).bg(palette.selection))
            .label(""),
        rows[3],
    );
}

fn draw_players(frame: &mut Frame, app: &App, area: Rect) {
    let palette = app.palette;
    let items: Vec<ListItem> = app
        .players
        .iter()
        .map(|p| {
            let selected = app.selected_id.as_deref() == Some(p.id.as_str());
            ListItem::new(vec![
                Line::from(format!("{} {}", if selected { "▶" } else { " " }, p.name)),
                Line::styled(
                    format!(
                        "  {} · {}",
                        if p.available {
                            p.state.as_str()
                        } else {
                            "unavailable"
                        },
                        p.volume.map_or("volume —".into(), |v| format!("vol {v}%"))
                    ),
                    Style::default().fg(palette.secondary),
                ),
            ])
        })
        .collect();
    let mut state = ListState::default().with_selected(
        (!app.players.is_empty())
            .then_some(app.player_cursor.min(app.players.len().saturating_sub(1))),
    );
    frame.render_stateful_widget(
        List::new(items)
            .block(panel(
                palette,
                " PLAYERS · Enter to select ",
                app.focus == Focus::Players,
            ))
            .highlight_style(Style::default().bg(palette.selection).fg(palette.accent))
            .highlight_symbol("› "),
        area,
        &mut state,
    );
}

/// Track rows shared by the queue and search panes.
fn track_items<'a>(tracks: &'a [TrackView], palette: Palette, numbered: bool) -> Vec<ListItem<'a>> {
    tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let position = if numbered {
                format!("{:>3}  ", index + 1)
            } else {
                String::new()
            };
            ListItem::new(vec![
                Line::from(format!(
                    "{position}{}  {}",
                    track.title,
                    duration(track.duration)
                )),
                Line::styled(
                    format!("{}{}", " ".repeat(position.len()), track.artist),
                    Style::default().fg(palette.secondary),
                ),
            ])
        })
        .collect()
}

fn draw_list(
    frame: &mut Frame,
    area: Rect,
    block: Block,
    items: Vec<ListItem>,
    cursor: usize,
    palette: Palette,
    empty: &str,
) {
    if items.is_empty() {
        frame.render_widget(Paragraph::new(empty).block(block), area);
        return;
    }
    let mut state =
        ListState::default().with_selected(Some(cursor.min(items.len().saturating_sub(1))));
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(Style::default().bg(palette.selection))
            .highlight_symbol("› "),
        area,
        &mut state,
    );
}

fn draw_queue(frame: &mut Frame, app: &App, area: Rect) {
    let palette = app.palette;
    let title = format!(" QUEUE · {} ", count(app.queue.len(), "item"));
    draw_list(
        frame,
        area,
        panel(palette, &title, app.focus == Focus::Queue),
        track_items(&app.queue, palette, true),
        app.queue_cursor,
        palette,
        "  Queue is empty or not loaded",
    );
}

fn draw_search(frame: &mut Frame, app: &App, area: Rect) {
    let palette = app.palette;
    let title = format!(" SEARCH · {} ", count(app.results.len(), "result"));
    draw_list(
        frame,
        area,
        panel(palette, &title, app.focus == Focus::Search || app.editing),
        track_items(&app.results, palette, false),
        app.search_cursor,
        palette,
        "  / search for tracks across providers",
    );
}

fn progress(app: &App) -> f64 {
    if app.duration > 0.0 && app.elapsed.is_finite() && app.duration.is_finite() {
        (app.elapsed / app.duration).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn transport(state: &str) -> &'static str {
    match state {
        "playing" => "▶",
        "paused" => "⏸",
        _ => "■",
    }
}

/// Advance the visualizer for this frame and report why it is empty when it
/// is. Bars are only ever drawn from decoded samples this process is playing.
fn spectrum(app: &mut App, width: u16) -> Option<String> {
    let (bars, _, _) = crate::visualizer::columns(width);
    let now = std::time::Instant::now();
    let captured = app.spectrum.as_ref().map(|analyzer| analyzer.capture(now));
    app.visualizer.update(
        match captured {
            Some(Ok(bands)) => Some(bands),
            _ => None,
        },
        bars,
        now,
    );
    match captured {
        Some(Ok(_)) => None,
        Some(Err("no local audio")) => Some(app.silence()),
        Some(Err(reason)) => Some(reason.into()),
        None => Some("local audio is off · Local Matui is not a speaker this run".into()),
    }
}

/// The visualizer replacing the browser/queue pane.
fn draw_spectrum_panel(frame: &mut Frame, app: &mut App, area: Rect) {
    let palette = app.palette;
    let block = panel(palette, " SPECTRUM · Local Matui's own output ", true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
    let reason = spectrum(app, rows[0].width);
    crate::visualizer::render(frame, rows[0], palette, &app.visualizer, reason.as_deref());
    frame.render_widget(
        Paragraph::new(ruler(app, rows[1].width, reason.is_some()))
            .style(Style::default().fg(palette.secondary)),
        rows[1],
    );
}

fn ruler(app: &App, width: u16, empty: bool) -> String {
    let rate = app.spectrum.as_ref().map_or(0, |a| a.rate());
    if empty || rate == 0 {
        return String::new();
    }
    crate::visualizer::scale(width, rate)
}

/// The whole terminal: bars over a compact now-playing line.
fn draw_visualizer(frame: &mut Frame, app: &mut App, area: Rect) {
    let palette = app.palette;
    let rows = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    let reason = spectrum(app, rows[0].width);
    crate::visualizer::render(frame, rows[0], palette, &app.visualizer, reason.as_deref());
    frame.render_widget(
        Paragraph::new(ruler(app, rows[1].width, reason.is_some()))
            .style(Style::default().fg(palette.secondary)),
        rows[1],
    );
    let state = app
        .players
        .iter()
        .find(|p| Some(&p.id) == app.selected_id.as_ref());
    let head = Layout::horizontal([Constraint::Min(10), Constraint::Length(16)]).split(rows[2]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", transport(state.map_or("", |p| p.state.as_str()))),
                Style::default().fg(palette.accent),
            ),
            Span::styled(
                app.title.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ])),
        head[0],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{} / {} ",
            duration(app.elapsed),
            duration(app.duration)
        ))
        .alignment(ratatui::layout::Alignment::Right),
        head[1],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "   {}{}",
            app.artist,
            state
                .and_then(|p| p.volume)
                .map_or(String::new(), |v| format!("  ·  vol {v}%"))
        ))
        .style(Style::default().fg(palette.secondary)),
        rows[3],
    );
    frame.render_widget(
        Gauge::default()
            .ratio(progress(app))
            .gauge_style(Style::default().fg(palette.accent).bg(palette.selection))
            .label(""),
        rows[4],
    );
    frame.render_widget(
        Paragraph::new(
            "v panel · Esc close · Space/p pause · </> track · +/- vol · z shuffle · ? all keys",
        )
        .style(Style::default().fg(palette.secondary)),
        rows[5],
    );
}

fn panel(palette: Palette, title: &str, active: bool) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if active {
            palette.accent
        } else {
            palette.secondary
        }))
}

fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

pub fn duration(seconds: f64) -> String {
    let seconds = if seconds.is_finite() {
        seconds.max(0.0) as u64
    } else {
        0
    };
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
