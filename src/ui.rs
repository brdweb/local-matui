use ratatui::{
    layout::{Constraint, Layout},
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
    Toggle,
    Next,
    Previous,
    Volume(i8),
    Seek(i8),
    Play(String),
    Enqueue(String),
}

impl App {
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
        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::F(2) => Action::OpenSettings,
            KeyCode::Char('?') | KeyCode::F(1) => {
                self.menu = Some(crate::controls::Menu::new(self));
                Action::None
            }
            KeyCode::Char('/') => {
                self.editing = true;
                self.focus = Focus::Search;
                self.query.clear();
                Action::None
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Players => Focus::Queue,
                    Focus::Queue => Focus::Search,
                    Focus::Search => Focus::Players,
                };
                Action::None
            }
            KeyCode::Esc => {
                self.focus = Focus::Queue;
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Up | KeyCode::Char('k') => {
                let down = matches!(key.code, KeyCode::Down | KeyCode::Char('j'));
                let (cursor, len) = match self.focus {
                    Focus::Players => (&mut self.player_cursor, self.players.len()),
                    Focus::Queue => (&mut self.queue_cursor, self.queue.len()),
                    Focus::Search => (&mut self.search_cursor, self.results.len()),
                };
                *cursor = if down {
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
            KeyCode::Char(' ') => Action::Toggle,
            KeyCode::Char('n') => Action::Next,
            KeyCode::Char('p') => Action::Previous,
            KeyCode::Char('s') => Action::Command(crate::controls::Command::Player {
                name: "stop",
                args: serde_json::json!({}),
            }),
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
            KeyCode::Char('+') | KeyCode::Char('=') => Action::Volume(5),
            KeyCode::Char('-') => Action::Volume(-5),
            KeyCode::Left => Action::Seek(-10),
            KeyCode::Right => Action::Seek(10),
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
    Queue,
    Search,
}

pub struct App {
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
            Paragraph::new("MATUI\nResize to 50 x 16\nq: quit")
                .style(Style::default().fg(palette.accent)),
            area,
        );
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(5),
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
                "  MATUI  ",
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
    let now = Layout::vertical([Constraint::Length(3), Constraint::Length(2)]).split(rows[1]);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                format!("  {}", app.title),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Line::styled(
                format!("  {}", app.artist),
                Style::default().fg(palette.secondary),
            ),
        ])
        .block(Block::default().title(" NOW PLAYING ")),
        now[0],
    );
    let ratio = if app.duration > 0.0 && app.elapsed.is_finite() && app.duration.is_finite() {
        (app.elapsed / app.duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    frame.render_widget(
        Gauge::default()
            .ratio(ratio)
            .gauge_style(Style::default().fg(palette.accent).bg(palette.selection))
            .label(format!(
                "{} / {}",
                duration(app.elapsed),
                duration(app.duration)
            )),
        now[1],
    );
    let cols =
        Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)]).split(rows[2]);
    let player_items: Vec<ListItem> = app
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
        List::new(player_items)
            .block(panel(
                palette,
                " PLAYERS · Enter to select ",
                app.focus == Focus::Players,
            ))
            .highlight_style(Style::default().bg(palette.selection).fg(palette.accent))
            .highlight_symbol("› "),
        cols[0],
        &mut state,
    );
    let search = app.focus == Focus::Search || app.editing;
    let tracks = if search { &app.results } else { &app.queue };
    let cursor = if search {
        app.search_cursor
    } else {
        app.queue_cursor
    };
    let title = if search {
        format!(" SEARCH · {} results ", tracks.len())
    } else {
        let shuffle = if app.queue_details["shuffle_enabled"] == true {
            "on"
        } else {
            "off"
        };
        let repeat = match app.queue_details["repeat_mode"].as_str() {
            Some("all") => "all",
            Some("one") => "one",
            _ => "off",
        };
        format!(
            " QUEUE · {} items · shuffle {shuffle} · repeat {repeat} ",
            tracks.len()
        )
    };
    let items: Vec<ListItem> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            ListItem::new(vec![
                Line::from(format!(
                    "{:>3}  {}  {}",
                    i + 1,
                    t.title,
                    duration(t.duration)
                )),
                Line::styled(
                    format!("     {}", t.artist),
                    Style::default().fg(palette.secondary),
                ),
            ])
        })
        .collect();
    if tracks.is_empty() {
        frame.render_widget(
            Paragraph::new(if search {
                "  / search for tracks across providers"
            } else {
                "  Queue is empty or not loaded"
            })
            .block(panel(palette, &title, app.focus != Focus::Players)),
            cols[1],
        );
    } else {
        let mut state =
            ListState::default().with_selected(Some(cursor.min(tracks.len().saturating_sub(1))));
        frame.render_stateful_widget(
            List::new(items)
                .block(panel(palette, &title, app.focus != Focus::Players))
                .highlight_style(Style::default().bg(palette.selection))
                .highlight_symbol("› "),
            cols[1],
            &mut state,
        );
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
    frame.render_widget(Paragraph::new("Tab pane · ↑↓/jk move · Space pause · n/p skip · +/- volume · ←→ seek\n/ search · Enter play* · a enqueue · Esc queue · r refresh · F2 settings · ? controls · q quit (*replaces queue)")
        .style(Style::default().fg(palette.secondary)), rows[4]);
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

pub fn duration(seconds: f64) -> String {
    let seconds = if seconds.is_finite() {
        seconds.max(0.0) as u64
    } else {
        0
    };
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
