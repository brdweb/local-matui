use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use local_matui::{
    music::{Browser, Kind, Media, Target},
    ui::{Action, App, Focus, PlayerView, TrackView},
};
use serde_json::json;

fn press(app: &mut App, key: KeyCode) -> Action {
    app.key(KeyEvent::new(key, KeyModifiers::NONE))
}
fn track() -> Media {
    Media::parse(
        &json!({"name":"Fixture song","uri":"library://track/1","media_type":"track"}),
        "",
    )
}
fn connected() -> App {
    App {
        connected: true,
        selected_id: Some("speaker".into()),
        players: vec![PlayerView {
            id: "speaker".into(),
            name: "Fixture speaker".into(),
            available: true,
            ..Default::default()
        }],
        focus: Focus::Music,
        ..Default::default()
    }
}

#[test]
fn navigation_is_read_only_and_back_rejects_late_responses() {
    let mut app = App::default();
    press(&mut app, KeyCode::Char('b'));
    let action = press(&mut app, KeyCode::Enter);
    assert!(matches!(
        action,
        Action::Browse {
            target: Target::Library {
                kind: Kind::Playlists,
                ..
            },
            ..
        }
    ));
    let generation = app.music.generation;
    press(&mut app, KeyCode::Backspace);
    app.music.apply(generation, Ok((vec![track()], None)));
    assert_eq!(app.music.page.target, Target::Home);
    assert!(!app.music.loading);
    assert!(app.menu.is_none());
}

#[test]
fn collections_open_and_playback_menu_identifies_speaker_and_queue_behavior() {
    let mut app = connected();
    let album = Media::parse(
        &json!({"name":"Fixture album","item_id":"album1","provider":"library","media_type":"album","uri":"library://album/album1"}),
        "",
    );
    app.music.page.items = vec![album];
    assert!(matches!(
        press(&mut app, KeyCode::Enter),
        Action::Browse {
            target: Target::Album { .. },
            ..
        }
    ));
    app.music.back();
    assert_eq!(press(&mut app, KeyCode::Char('P')), Action::None);
    assert!(app.menu.as_ref().unwrap().title.contains("Fixture speaker"));
    assert!(app.menu.as_ref().unwrap().entries[0]
        .label
        .contains("replace queue"));
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        Action::Play("library://album/album1".into())
    );
    app.music.page.items = vec![track()];
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Down);
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        Action::PlayNext("library://track/1".into())
    );
    assert_eq!(
        press(&mut app, KeyCode::Char('a')),
        Action::Enqueue("library://track/1".into())
    );
    assert_eq!(
        press(&mut app, KeyCode::Char('N')),
        Action::PlayNext("library://track/1".into())
    );
}

#[test]
fn unavailable_media_and_player_changes_cannot_submit_playback() {
    let mut app = connected();
    let mut media = track();
    media.available = false;
    app.music.page.items = vec![media];
    assert_eq!(press(&mut app, KeyCode::Enter), Action::None);
    assert!(app.menu.is_none());
    app.music.page.items = vec![track()];
    press(&mut app, KeyCode::Enter);
    app.connected = false;
    assert_eq!(press(&mut app, KeyCode::Enter), Action::None);
    assert_eq!(press(&mut app, KeyCode::Char('a')), Action::None);
}

#[test]
fn search_collections_open_in_browser_and_tracks_offer_actions() {
    let mut app = connected();
    app.focus = Focus::Search;
    app.results = vec![TrackView {
        media: Some(Media::parse(
            &json!({"name":"Playlist","media_type":"playlist","item_id":"list","provider":"library","uri":"library://playlist/list"}),
            "",
        )),
        ..Default::default()
    }];
    assert!(matches!(
        press(&mut app, KeyCode::Enter),
        Action::Browse {
            target: Target::Playlist { .. },
            ..
        }
    ));
    assert!(app.focus == Focus::Music);
    app.focus = Focus::Search;
    app.results[0].media = Some(track());
    assert_eq!(press(&mut app, KeyCode::Enter), Action::None);
    assert!(app.menu.is_some());
}

#[test]
fn folders_are_not_playable_and_terminal_controls_are_removed() {
    let folder = Media::parse(
        &json!({"name":"Folder\n\u{1b}","media_type":"folder","path":"provider://browse/abc","uri":"provider://folder/abc"}),
        "",
    );
    assert_eq!(folder.title, "Folder");
    assert!(!folder.playable);
    assert_eq!(
        folder.open,
        Some(Target::Providers {
            path: Some("provider://browse/abc".into())
        })
    );
}

#[test]
fn failed_listing_can_retry_and_pagination_back_restores_position() {
    let mut browser = Browser::default();
    let target = Target::Library {
        kind: Kind::Tracks,
        offset: 0,
        favorite: false,
    };
    browser.navigate(target.clone(), "Tracks".into());
    browser.apply(browser.generation, Err("Unavailable".into()));
    assert!(!browser.loading);
    assert_eq!(browser.error, "Unavailable");
    browser.reload();
    let next = Target::Library {
        kind: Kind::Tracks,
        offset: 100,
        favorite: false,
    };
    browser.apply(
        browser.generation,
        Ok((vec![track(), track()], Some(next.clone()))),
    );
    browser.page.cursor = 1;
    browser.navigate(next, "Tracks".into());
    browser.back();
    assert_eq!(browser.page.target, target);
    assert_eq!(browser.page.cursor, 1);
    assert!(browser.error.is_empty());
}
