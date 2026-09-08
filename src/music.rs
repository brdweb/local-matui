//! Read-only music navigation, kept separate from explicit playback actions.
use crate::{
    api::ApiClient,
    ui::{Action, App, Focus},
};
use anyhow::{anyhow, Result};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use serde_json::{json, Value};

pub const PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Home,
    Library {
        kind: Kind,
        offset: usize,
        favorite: bool,
    },
    Album {
        id: String,
        provider: String,
    },
    Playlist {
        id: String,
        provider: String,
    },
    Artist {
        id: String,
        provider: String,
    },
    ArtistTracks {
        id: String,
        provider: String,
    },
    Providers {
        path: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Playlists,
    Albums,
    Artists,
    Tracks,
    Radio,
}
impl Kind {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Playlists => "playlists",
            Self::Albums => "albums",
            Self::Artists => "artists",
            Self::Tracks => "tracks",
            Self::Radio => "radios",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Playlists => "Playlists",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Tracks => "Tracks",
            Self::Radio => "Radio",
        }
    }
    fn media_type(self) -> &'static str {
        match self {
            Self::Playlists => "playlist",
            Self::Albums => "album",
            Self::Artists => "artist",
            Self::Tracks => "track",
            Self::Radio => "radio",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Media {
    pub title: String,
    pub detail: String,
    pub uri: String,
    pub kind: String,
    pub playable: bool,
    pub available: bool,
    pub open: Option<Target>,
}
impl Media {
    pub fn folder(title: &str, target: Target) -> Self {
        Self {
            title: title.into(),
            detail: "Enter to browse".into(),
            uri: String::new(),
            kind: "folder".into(),
            playable: false,
            available: true,
            open: Some(target),
        }
    }
    pub fn parse(v: &Value, hint: &str) -> Self {
        let value = |key: &str| v[key].as_str().unwrap_or_default().to_owned();
        let id = value("item_id");
        let provider = value("provider");
        let kind = v["media_type"].as_str().unwrap_or(hint).to_owned();
        let uri = value("uri");
        let open = if kind == "folder" {
            v["path"]
                .as_str()
                .filter(|p| !p.is_empty())
                .map(|p| Target::Providers {
                    path: Some(p.into()),
                })
        } else if !id.is_empty() && !provider.is_empty() {
            match kind.as_str() {
                "album" => Some(Target::Album { id, provider }),
                "playlist" => Some(Target::Playlist { id, provider }),
                "artist" => Some(Target::Artist { id, provider }),
                _ => None,
            }
        } else {
            None
        };
        let artists = v["artists"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a["name"].as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let detail = if artists.is_empty() {
            clean(&format!("{} · {}", kind, value("provider")))
        } else {
            clean(&format!("{artists} · {kind}"))
        };
        Self {
            title: clean(&value("name")),
            detail,
            uri: uri.clone(),
            kind: kind.clone(),
            available: v["available"] != false,
            playable: !uri.is_empty()
                && v["is_playable"].as_bool().unwrap_or(matches!(
                    kind.as_str(),
                    "track"
                        | "album"
                        | "artist"
                        | "playlist"
                        | "radio"
                        | "audiobook"
                        | "podcast"
                        | "podcast_episode"
                        | "audio_source"
                )),
            open,
        }
    }
}
fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(512).collect()
}

/// Fictional catalog for offline previews; no server or audio access.
pub fn demo_listing(target: &Target) -> Vec<Media> {
    let kind = match target {
        Target::Home => return Page::default().items,
        Target::Library { kind, .. } => kind.media_type(),
        Target::Artist { .. } => "album",
        Target::Providers { .. } => "playlist",
        _ => "track",
    };
    vec![Media::parse(
        &json!({"name":format!("Sample {kind} — offline preview"),"item_id":"sample","provider":"demo","uri":format!("demo://{kind}/sample"),"media_type":kind,"artists":[{"name":"Fictional artist"}]}),
        kind,
    )]
}

#[derive(Clone)]
pub struct Page {
    pub target: Target,
    pub title: String,
    pub items: Vec<Media>,
    pub cursor: usize,
    pub next: Option<Target>,
}
impl Default for Page {
    fn default() -> Self {
        let mut items = vec![];
        for kind in [
            Kind::Playlists,
            Kind::Albums,
            Kind::Artists,
            Kind::Tracks,
            Kind::Radio,
        ] {
            items.push(Media::folder(
                kind.label(),
                Target::Library {
                    kind,
                    offset: 0,
                    favorite: false,
                },
            ));
        }
        items.push(Media::folder(
            "Favorite tracks",
            Target::Library {
                kind: Kind::Tracks,
                offset: 0,
                favorite: true,
            },
        ));
        items.push(Media::folder(
            "Browse music providers",
            Target::Providers { path: None },
        ));
        Self {
            target: Target::Home,
            title: "Music library".into(),
            items,
            cursor: 0,
            next: None,
        }
    }
}

#[derive(Default)]
pub struct Browser {
    pub page: Page,
    pub history: Vec<Page>,
    pub generation: u64,
    pub loading: bool,
    pub error: String,
}
impl Browser {
    pub fn navigate(&mut self, target: Target, title: String) -> Action {
        if self.history.len() == 32 {
            self.history.remove(0);
        }
        self.history.push(self.page.clone());
        self.page = Page {
            target,
            title,
            items: vec![],
            cursor: 0,
            next: None,
        };
        self.reload()
    }
    pub fn reload(&mut self) -> Action {
        self.generation += 1;
        self.error.clear();
        if self.page.target == Target::Home {
            self.page = Page::default();
            self.loading = false;
            return Action::None;
        }
        self.loading = true;
        Action::Browse {
            generation: self.generation,
            target: self.page.target.clone(),
        }
    }
    pub fn back(&mut self) {
        self.generation += 1;
        self.loading = false;
        self.error.clear();
        self.page = self.history.pop().unwrap_or_default();
    }
    pub fn apply(
        &mut self,
        generation: u64,
        result: std::result::Result<(Vec<Media>, Option<Target>), String>,
    ) {
        if generation != self.generation {
            return;
        }
        self.loading = false;
        match result {
            Ok((items, next)) => {
                self.page.items = items;
                self.page.next = next;
                self.page.cursor = self
                    .page
                    .cursor
                    .min(self.page.items.len().saturating_sub(1));
            }
            Err(error) => {
                self.error = error;
                self.page.items.clear();
                self.page.next = None;
            }
        }
    }
}

impl ApiClient {
    pub async fn browse(&self, target: &Target) -> Result<(Vec<Media>, Option<Target>)> {
        let (command, args, hint) = match target {
            Target::Home => return Ok((Page::default().items, None)),
            Target::Library {
                kind,
                offset,
                favorite,
            } => (
                format!("music/{}/library_items", kind.endpoint()),
                json!({"limit":PAGE_SIZE,"offset":offset,"order_by":"sort_name","favorite":if *favorite {Some(true)} else {None}}),
                kind.media_type(),
            ),
            Target::Album { id, provider } => (
                "music/albums/album_tracks".into(),
                json!({"item_id":id,"provider_instance_id_or_domain":provider}),
                "track",
            ),
            Target::Playlist { id, provider } => (
                "music/playlists/playlist_tracks".into(),
                json!({"item_id":id,"provider_instance_id_or_domain":provider}),
                "track",
            ),
            Target::Artist { id, provider } => (
                "music/artists/artist_albums".into(),
                json!({"item_id":id,"provider_instance_id_or_domain":provider}),
                "album",
            ),
            Target::ArtistTracks { id, provider } => (
                "music/artists/artist_tracks".into(),
                json!({"item_id":id,"provider_instance_id_or_domain":provider}),
                "track",
            ),
            Target::Providers { path } => ("music/browse".into(), json!({"path":path}), ""),
        };
        let value = self.command(&command, args).await?;
        let rows = value
            .as_array()
            .ok_or_else(|| anyhow!("Invalid music listing"))?;
        let mut items: Vec<_> = rows.iter().map(|v| Media::parse(v, hint)).collect();
        if let Target::Artist { id, provider } = target {
            items.insert(
                0,
                Media::folder(
                    "Top tracks",
                    Target::ArtistTracks {
                        id: id.clone(),
                        provider: provider.clone(),
                    },
                ),
            );
        }
        let next = match target {
            Target::Library {
                kind,
                offset,
                favorite,
            } if rows.len() == PAGE_SIZE => Some(Target::Library {
                kind: *kind,
                offset: offset + PAGE_SIZE,
                favorite: *favorite,
            }),
            _ => None,
        };
        Ok((items, next))
    }
}

pub fn choose(app: &mut App, media: &Media) -> Action {
    let Some(player) = app
        .players
        .iter()
        .find(|p| Some(&p.id) == app.selected_id.as_ref() && p.available && app.connected)
    else {
        app.status = "Select a speaker in Players first, then choose music".into();
        return Action::None;
    };
    if !media.available || !media.playable {
        app.status = "This item is not available for playback".into();
        return Action::None;
    }
    app.menu = Some(crate::controls::Menu {
        player: Some(player.id.clone()),
        title: format!("Play on {} · {}", player.name, media.title),
        entries: vec![
            (
                "Play now (replace queue)".into(),
                Action::Play(media.uri.clone()),
            ),
            ("Play next".into(), Action::PlayNext(media.uri.clone())),
            ("Add to queue".into(), Action::Enqueue(media.uri.clone())),
        ],
        cursor: 0,
        prompt: None,
        error: String::new(),
    });
    Action::None
}

pub fn key(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Backspace => {
            app.music.back();
            Some(Action::None)
        }
        KeyCode::Char('r') => Some(app.music.reload()),
        KeyCode::Char(']') => {
            if app.music.loading {
                return Some(Action::None);
            }
            Some(if let Some(target) = app.music.page.next.clone() {
                app.music.navigate(target, app.music.page.title.clone())
            } else {
                Action::None
            })
        }
        KeyCode::Enter | KeyCode::Char('P') | KeyCode::Char('a') | KeyCode::Char('N') => {
            if app.music.loading {
                return Some(Action::None);
            }
            let Some(media) = app.music.page.items.get(app.music.page.cursor).cloned() else {
                return Some(Action::None);
            };
            if key.code == KeyCode::Enter {
                if let Some(target) = media.open.clone() {
                    return Some(app.music.navigate(target, media.title));
                }
            }
            if matches!(key.code, KeyCode::Enter | KeyCode::Char('P')) {
                return Some(choose(app, &media));
            }
            if !app.connected
                || !app
                    .players
                    .iter()
                    .any(|p| p.available && Some(&p.id) == app.selected_id.as_ref())
            {
                app.status = "Select an available speaker first".into();
                return Some(Action::None);
            }
            Some(if media.available && media.playable {
                if key.code == KeyCode::Char('a') {
                    Action::Enqueue(media.uri)
                } else {
                    Action::PlayNext(media.uri)
                }
            } else {
                app.status = "This item is not available for playback".into();
                Action::None
            })
        }
        _ => None,
    }
}

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let browser = &app.music;
    let palette = app.palette;
    let paging = match browser.page.target {
        Target::Library { offset, .. } => format!(
            " · page {}{}",
            offset / PAGE_SIZE + 1,
            if browser.page.next.is_some() {
                " · ] next"
            } else {
                ""
            }
        ),
        _ => String::new(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" MUSIC · {}{} ", browser.page.title, paging))
        .border_style(Style::default().fg(if app.focus == Focus::Music {
            palette.accent
        } else {
            palette.secondary
        }));
    if browser.loading || !browser.error.is_empty() || browser.page.items.is_empty() {
        let message = if browser.loading {
            "Loading music…"
        } else if !browser.error.is_empty() {
            &browser.error
        } else {
            "No items here. Try another category, browse providers, or / search."
        };
        frame.render_widget(Paragraph::new(format!("{message}\n\nEnter opens collections · P chooses playback\nBackspace goes back · r retries")).wrap(ratatui::widgets::Wrap {trim:false}).block(block),area);
        return;
    }
    let items = browser.page.items.iter().map(|m| {
        ListItem::new(vec![
            Line::from(format!(
                "{} {}{}",
                if m.open.is_some() { "›" } else { "♪" },
                m.title,
                if m.available { "" } else { " [unavailable]" }
            )),
            Line::styled(
                format!("  {}", m.detail),
                Style::default().fg(palette.secondary),
            ),
        ])
    });
    let mut state = ListState::default().with_selected(Some(browser.page.cursor));
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_symbol("▸ ")
            .highlight_style(Style::default().fg(palette.accent).bg(palette.selection)),
        area,
        &mut state,
    );
}
