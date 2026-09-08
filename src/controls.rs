//! Playback controls only: no provider, user, library-management or server-admin commands.
use crate::ui::{Action, App};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout},
    style::Style,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Player {
        name: &'static str,
        args: Value,
    },
    Queue {
        id: String,
        name: &'static str,
        args: Value,
    },
    Transfer {
        source: String,
        target: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub label: String,
    pub command: Command,
    pub argument: &'static str,
    pub value: String,
    pub kind: InputKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputKind {
    Text,
    Number { min: f64, max: f64, integer: bool },
}

impl Prompt {
    pub fn submit(&self) -> Result<Command, &'static str> {
        let value = match self.kind {
            InputKind::Text if self.value.trim().is_empty() => return Err("Enter a value"),
            InputKind::Text => json!(self.value.trim()),
            InputKind::Number { min, max, integer } => {
                let n: f64 = self
                    .value
                    .trim()
                    .parse()
                    .map_err(|_| "Enter a number within the displayed range")?;
                if !n.is_finite() || n < min || n > max || (integer && n.fract() != 0.0) {
                    return Err("Enter a number within the displayed range");
                }
                if integer {
                    json!(n as i64)
                } else {
                    json!(n)
                }
            }
        };
        let mut command = self.command.clone();
        match &mut command {
            Command::Player { args, .. } | Command::Queue { args, .. } => {
                args[self.argument] = value
            }
            _ => return Err("Invalid input action"),
        }
        Ok(command)
    }
}

pub struct Menu {
    pub prompt: Option<Prompt>,
    pub error: String,
    pub player: Option<String>,
    pub title: String,
    pub entries: Vec<(String, Action)>,
    pub cursor: usize,
}

impl Menu {
    pub fn new(app: &App) -> Self {
        let mut menu = Self {
            prompt: None,
            error: String::new(),
            player: app.selected_id.clone(),
            title: "Select a connected player first".into(),
            entries: vec![],
            cursor: 0,
        };
        let Some(player) = app
            .players
            .iter()
            .find(|p| Some(&p.id) == app.selected_id.as_ref() && p.available && app.connected)
        else {
            return menu;
        };
        menu.title = format!("Controls · {}", player.name);
        let p = &player.details;
        let mut add_player = |label: String, name, args| {
            menu.entries
                .push((label, Action::Command(Command::Player { name, args })))
        };
        for (label, name) in [
            ("Play / resume", "play"),
            ("Pause", "pause"),
            ("Stop", "stop"),
            ("Next track", "next"),
            ("Previous track", "previous"),
        ] {
            add_player(label.into(), name, json!({}));
        }
        if let Some(muted) = p["volume_muted"].as_bool() {
            add_player(
                if muted { "Unmute" } else { "Mute" }.into(),
                "volume_mute",
                json!({"muted":!muted}),
            );
        }
        if let Some(powered) = p["powered"].as_bool() {
            add_player(
                if powered { "Power off" } else { "Power on" }.into(),
                "power",
                json!({"powered":!powered}),
            );
        }
        for (label, name) in [
            ("Volume up", "volume_up"),
            ("Volume down", "volume_down"),
            ("Group volume up", "group_volume_up"),
            ("Group volume down", "group_volume_down"),
        ] {
            add_player(label.into(), name, json!({}));
        }
        if let Some(muted) = p["group_volume_muted"].as_bool() {
            add_player(
                if muted { "Unmute group" } else { "Mute group" }.into(),
                "group_volume_mute",
                json!({"muted":!muted}),
            );
        }
        add_player("Leave player group".into(), "ungroup", json!({}));
        for target in app
            .players
            .iter()
            .filter(|other| other.available && other.id != player.id)
        {
            if p["can_group_with"]
                .as_array()
                .is_some_and(|ids| ids.contains(&json!(target.id)))
            {
                add_player(
                    format!("Join group led by {}", target.name),
                    "group",
                    json!({"target_player":target.id}),
                );
            }
            if p["group_members"]
                .as_array()
                .is_some_and(|ids| ids.contains(&json!(target.id)))
            {
                add_player(
                    format!("Remove {} from this group", target.name),
                    "set_members",
                    json!({"player_ids_to_remove":[target.id]}),
                );
            }
        }
        for (field, name, argument, label) in [
            ("source_list", "select_source", "source", "Source"),
            (
                "sound_mode_list",
                "select_sound_mode",
                "sound_mode",
                "Sound mode",
            ),
        ] {
            for item in p[field]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|v| v["passive"] != true)
            {
                if let Some(id) = item["id"].as_str() {
                    add_player(
                        format!("{label}: {}", clean(&item["name"])),
                        name,
                        json!({argument:id}),
                    );
                }
            }
        }
        for option in p["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["read_only"] != true)
        {
            let Some(key) = option["key"].as_str() else {
                continue;
            };
            let label = clean(&option["name"]);
            if let Some(value) = option["value"].as_bool() {
                add_player(
                    format!("{label}: {}", !value),
                    "set_option",
                    json!({"option_key":key,"option_value":!value}),
                );
            } else if let Some(options) = option["options"].as_array() {
                for choice in options {
                    add_player(
                        format!("{label}: {}", clean(&choice["name"])),
                        "set_option",
                        json!({"option_key":key,"option_value":choice["value"]}),
                    );
                }
            } else if let Some(value) = option["value"].as_f64() {
                let step = option["step"].as_f64().unwrap_or(1.0);
                for delta in [-step, step] {
                    let next = value + delta;
                    if !next.is_finite()
                        || option["min_value"].as_f64().is_some_and(|v| next < v)
                        || option["max_value"].as_f64().is_some_and(|v| next > v)
                    {
                        continue;
                    }
                    let next = if option["type"] == "integer" {
                        json!(next as i64)
                    } else {
                        json!(next)
                    };
                    add_player(
                        format!("{label}: {next}"),
                        "set_option",
                        json!({"option_key":key,"option_value":next}),
                    );
                }
            }
        }
        for minutes in [15, 30, 60, 90] {
            add_player(
                format!("Sleep timer: {minutes} minutes"),
                "sleep_timer/set",
                json!({"seconds":minutes*60}),
            );
        }
        add_player("Cancel sleep timer".into(), "sleep_timer/clear", json!({}));
        if !app.queue_id.is_empty() {
            let q = &app.queue_details;
            let mut add_queue = |label: String, name, args| {
                menu.entries.push((
                    label,
                    Action::Command(Command::Queue {
                        id: app.queue_id.clone(),
                        name,
                        args,
                    }),
                ))
            };
            if q["is_dynamic"] != true {
                let shuffle = q["shuffle_enabled"].as_bool().unwrap_or(false);
                add_queue(
                    format!("Shuffle: {}", if shuffle { "off" } else { "on" }),
                    "shuffle",
                    json!({"shuffle_enabled":!shuffle}),
                );
                for mode in ["off", "one", "all"] {
                    add_queue(
                        format!("Repeat: {mode}"),
                        "repeat",
                        json!({"repeat_mode":mode}),
                    );
                }
            }
            for (label, field, name) in [
                ("Autoplay", "autoplay_enabled", "autoplay"),
                ("Crossfade", "crossfade_enabled", "crossfade"),
            ] {
                if let Some(value) = q[field].as_bool() {
                    add_queue(
                        format!("{label}: {}", if value { "off" } else { "on" }),
                        name,
                        json!({field:!value}),
                    );
                }
            }
            add_queue("Clear queue and stop playback".into(), "clear", json!({}));
            if let Some(item) = app.queue.get(app.queue_cursor).filter(|t| !t.id.is_empty()) {
                add_queue(
                    format!("Play queue item: {}", item.title),
                    "play_index",
                    json!({"index":item.id}),
                );
                add_queue(
                    format!("Remove queue item: {}", item.title),
                    "delete_item",
                    json!({"item_id_or_index":item.id}),
                );
                for (label, shift) in [("Move up", -1), ("Move down", 1), ("Move to next", 0)] {
                    add_queue(
                        format!("{label}: {}", item.title),
                        "move_item",
                        json!({"queue_item_id":item.id,"pos_shift":shift}),
                    );
                }
                add_queue(
                    format!("Move to end: {}", item.title),
                    "move_item_end",
                    json!({"queue_item_id":item.id}),
                );
            }
            if let Some(item) = app.results.get(app.search_cursor) {
                for (label, option) in [
                    ("Play now (replace)", "replace"),
                    ("Play next", "next"),
                    ("Add to queue", "add"),
                    ("Play immediately, keep queue", "play"),
                ] {
                    add_queue(
                        format!("{label}: {}", item.title),
                        "play_media",
                        json!({"media":item.uri,"option":option}),
                    );
                }
            }
            if matches!(
                q["current_item"]["media_item"]["media_type"].as_str(),
                Some("audiobook" | "podcast_episode")
            ) {
                for speed in [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
                    add_queue(
                        format!("Playback speed: {speed}x"),
                        "set_playback_speed",
                        json!({"speed":speed}),
                    );
                }
            }
            for target in app
                .players
                .iter()
                .filter(|p| p.available && Some(&p.id) != app.selected_id.as_ref())
            {
                menu.entries.push((
                    format!(
                        "Transfer queue/playback to {} (replaces destination)",
                        target.name
                    ),
                    Action::Command(Command::Transfer {
                        source: app.queue_id.clone(),
                        target: target.id.clone(),
                    }),
                ));
            }
        }
        for (label, name, argument, min, max) in [
            (
                "Set volume (0–100)",
                "volume_set",
                "volume_level",
                0.0,
                100.0,
            ),
            (
                "Set group volume (0–100)",
                "group_volume",
                "volume_level",
                0.0,
                100.0,
            ),
            (
                "Seek to seconds (0–86400)",
                "seek",
                "position",
                0.0,
                86400.0,
            ),
            (
                "Sleep timer in seconds (1–86400)",
                "sleep_timer/set",
                "seconds",
                1.0,
                86400.0,
            ),
        ] {
            menu.input(
                label.into(),
                Command::Player {
                    name,
                    args: json!({}),
                },
                argument,
                InputKind::Number {
                    min,
                    max,
                    integer: true,
                },
            );
        }
        for option in p["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["read_only"] != true && !v["options"].is_array())
        {
            if option["type"] == "string" {
                if let Some(key) = option["key"].as_str() {
                    menu.input(
                        format!("Set {}", clean(&option["name"])),
                        Command::Player {
                            name: "set_option",
                            args: json!({"option_key":key}),
                        },
                        "option_value",
                        InputKind::Text,
                    );
                }
            }
        }
        if !app.queue_id.is_empty() {
            for (label, option) in [
                ("Play media URI (replaces queue)", "replace"),
                ("Add media URI to queue", "add"),
                ("Play media URI next", "next"),
            ] {
                menu.input(
                    label.into(),
                    Command::Queue {
                        id: app.queue_id.clone(),
                        name: "play_media",
                        args: json!({"option":option}),
                    },
                    "media",
                    InputKind::Text,
                );
            }
        }
        menu
    }

    fn input(&mut self, label: String, command: Command, argument: &'static str, kind: InputKind) {
        self.entries.push((
            label.clone(),
            Action::Prompt(Prompt {
                label,
                command,
                argument,
                kind,
                value: String::new(),
            }),
        ));
    }
}

fn clean(value: &Value) -> String {
    value
        .as_str()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect()
}

pub fn key(app: &mut App, key: KeyEvent) -> Action {
    let menu = app.menu.as_mut().unwrap();
    if let Some(prompt) = &mut menu.prompt {
        match key.code {
            KeyCode::Esc => {
                menu.prompt = None;
                menu.error.clear();
            }
            KeyCode::Enter => {
                if !app.connected
                    || menu.player != app.selected_id
                    || !app
                        .players
                        .iter()
                        .any(|p| p.available && Some(&p.id) == menu.player.as_ref())
                {
                    menu.error = "Player unavailable; cancel and reconnect".into();
                    return Action::None;
                }
                match prompt.submit() {
                    Ok(command) => {
                        app.menu = None;
                        return Action::Command(command);
                    }
                    Err(error) => menu.error = error.into(),
                }
            }
            KeyCode::Backspace => {
                prompt.value.pop();
            }
            KeyCode::Char(c) if !c.is_control() && prompt.value.len() < 2048 => {
                prompt.value.push(c)
            }
            _ => {}
        }
        return Action::None;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(1) => {
            app.menu = None;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            menu.cursor = (menu.cursor + 1).min(menu.entries.len().saturating_sub(1))
        }
        KeyCode::Up | KeyCode::Char('k') => menu.cursor = menu.cursor.saturating_sub(1),
        KeyCode::PageDown => {
            menu.cursor = (menu.cursor + 10).min(menu.entries.len().saturating_sub(1))
        }
        KeyCode::PageUp => menu.cursor = menu.cursor.saturating_sub(10),
        KeyCode::Home => menu.cursor = 0,
        KeyCode::End => menu.cursor = menu.entries.len().saturating_sub(1),
        KeyCode::Enter => {
            let action = if app.connected
                && menu.player == app.selected_id
                && app
                    .players
                    .iter()
                    .any(|p| Some(&p.id) == menu.player.as_ref() && p.available)
            {
                menu.entries
                    .get(menu.cursor)
                    .map(|(_, a)| a.clone())
                    .unwrap_or(Action::None)
            } else {
                app.status = "Player disconnected; reopen controls after reconnecting".into();
                Action::None
            };
            if let Action::Prompt(prompt) = action {
                menu.prompt = Some(prompt);
                return Action::None;
            }
            app.menu = None;
            return action;
        }
        _ => {}
    }
    Action::None
}

pub fn draw(frame: &mut Frame, app: &App) {
    let menu = app.menu.as_ref().unwrap();
    let palette = app.palette;
    frame.render_widget(
        Block::default().style(
            Style::default()
                .fg(palette.foreground)
                .bg(palette.background),
        ),
        frame.area(),
    );
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .margin(1)
    .split(frame.area());
    if let Some(prompt) = &menu.prompt {
        frame.render_widget(
            Paragraph::new(format!("{}\nEnter applies · Esc cancels", prompt.label)),
            rows[0],
        );
        frame.render_widget(
            Paragraph::new(format!("{}▏\n\n{}", prompt.value, menu.error))
                .wrap(ratatui::widgets::Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL)),
            rows[1],
        );
        return;
    }
    frame.render_widget(
        Paragraph::new(format!(
            "MATUI · {}\n↑↓/jk choose · PgUp/PgDn scroll · Enter applies · Esc returns",
            menu.title
        )),
        rows[0],
    );
    let mut state = ListState::default().with_selected(Some(menu.cursor));
    frame.render_stateful_widget(
        List::new(
            menu.entries
                .iter()
                .map(|(label, _)| ListItem::new(label.as_str())),
        )
        .block(Block::default().borders(Borders::ALL))
        .highlight_symbol("› ")
        .highlight_style(Style::default().fg(palette.accent).bg(palette.selection)),
        rows[1],
        &mut state,
    );
    frame.render_widget(Paragraph::new("Queue: Enter plays · Delete removes · Shift-J/K reorders\nPlayer capabilities vary; command errors are shown without retrying."), rows[2]);
}
