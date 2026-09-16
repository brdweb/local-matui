use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ma_tui::{
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
    // What you were in the middle of leads the home listing.
    let action = press(&mut app, KeyCode::Enter);
    assert!(matches!(
        action,
        Action::Browse {
            target: Target::InProgress,
            ..
        }
    ));
    // The shelves lead, in the order they are most reached for.
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Down);
    assert!(matches!(
        press(&mut app, KeyCode::Enter),
        Action::Browse {
            target: Target::UnplayedEpisodes,
            ..
        }
    ));
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Down);
    assert!(matches!(
        press(&mut app, KeyCode::Enter),
        Action::Browse {
            target: Target::RecentlyAdded,
            ..
        }
    ));
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Down);
    let action = press(&mut app, KeyCode::Enter);
    assert!(
        matches!(
            action,
            Action::Browse {
                target: Target::Library {
                    kind: Kind::Playlists,
                    ..
                },
                ..
            }
        ),
        "the libraries follow the shelves"
    );
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

/// Podcasts open into episodes; audiobooks deliberately do not, because MA
/// 2.10.2 has no chapter model to open into.
#[test]
fn podcasts_open_into_episodes_and_audiobooks_stay_a_single_item() {
    let podcast = Media::parse(
        &json!({"name":"A show","item_id":"p1","provider":"audiobookshelf",
                "media_type":"podcast","uri":"library://podcast/p1"}),
        "",
    );
    assert_eq!(
        podcast.open,
        Some(Target::Podcast {
            id: "p1".into(),
            provider: "audiobookshelf".into()
        })
    );
    assert!(podcast.playable, "a podcast can still be played whole");

    let audiobook = Media::parse(
        &json!({"name":"A book","item_id":"b1","provider":"audiobookshelf",
                "media_type":"audiobook","uri":"library://audiobook/b1"}),
        "",
    );
    assert_eq!(audiobook.open, None, "an audiobook has no chapter listing");
    assert!(audiobook.playable);
    assert_eq!(audiobook.id, "b1");
    assert_eq!(audiobook.provider, "audiobookshelf");
}

/// Progress is shown when the provider reports it, and nothing is invented
/// when it does not: unknown is not the same as unplayed.
#[test]
fn listening_progress_is_shown_only_when_the_server_reports_it() {
    let episode = |extra: serde_json::Value| {
        let mut item = json!({"name":"Episode 1","item_id":"e1","provider":"abs",
                              "media_type":"podcast_episode","uri":"library://podcast_episode/e1"});
        for (key, value) in extra.as_object().unwrap() {
            item[key] = value.clone();
        }
        Media::parse(&item, "podcast_episode")
    };

    let unknown = episode(json!({}));
    assert!(!unknown.fully_played);
    assert_eq!(unknown.resume_ms, None);
    assert!(
        !unknown.detail.contains("resume") && !unknown.detail.contains("played"),
        "an unreported progress state claims nothing: {}",
        unknown.detail
    );

    let finished = episode(json!({"fully_played": true}));
    assert!(finished.fully_played);
    assert!(finished.detail.contains("played"));

    let partway = episode(json!({"resume_position_ms": 724_000}));
    assert_eq!(partway.resume_ms, Some(724_000));
    assert!(
        partway.detail.contains("resume 12:04"),
        "minutes and seconds: {}",
        partway.detail
    );

    // An audiobook resume point runs to hours, so it is not shown as minutes.
    let deep = episode(json!({"resume_position_ms": 9_305_000}));
    assert!(
        deep.detail.contains("resume 2:35:05"),
        "hours are kept: {}",
        deep.detail
    );

    // A finished item says so rather than also offering a resume point.
    let both = episode(json!({"fully_played": true, "resume_position_ms": 5_000}));
    assert!(both.detail.contains("played") && !both.detail.contains("resume"));
}

/// Marking progress is a library edit, so it is offered without a speaker and
/// names the item the way Music Assistant expects.
#[test]
fn progress_can_be_marked_without_a_speaker_selected() {
    let episode = Media::parse(
        &json!({"name":"Episode 1","item_id":"e1","provider":"abs",
                "media_type":"podcast_episode","uri":"library://podcast_episode/e1"}),
        "",
    );
    // No speaker: playback entries are impossible, the progress ones are not.
    let mut app = App::default();
    assert_eq!(ma_tui::music::choose(&mut app, &episode), Action::None);
    let menu = app.menu.as_ref().expect("a progress menu still opens");
    assert!(menu.player.is_none());
    let labels: Vec<&str> = menu.entries.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, vec!["Mark as played", "Mark as not played"]);
    assert_eq!(
        menu.entries[0].action,
        Action::MarkPlayed {
            item: json!({
                "item_id": "e1",
                "provider": "abs",
                "name": "Episode 1",
                "media_type": "podcast_episode",
            }),
            played: true,
        },
        "the four fields ItemMapping requires, and no guesses beyond them"
    );

    // With a speaker, playback comes first and progress is still offered.
    let mut app = connected();
    ma_tui::music::choose(&mut app, &episode);
    let menu = app.menu.as_ref().unwrap();
    assert!(menu.entries[0].label.contains("Play now"));
    assert!(menu.entries.iter().any(|e| e.label == "Mark as played"));

    // A plain track keeps no listening position, so it is not offered one.
    let mut app = connected();
    ma_tui::music::choose(&mut app, &track());
    assert!(!app
        .menu
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .any(|e| e.label.starts_with("Mark")));
}

/// Progress events arrive while an audiobook plays, so re-reading the listing
/// is throttled and only happens where it would show.
#[test]
fn a_progress_event_refreshes_at_most_one_listing_at_a_time() {
    use std::time::{Duration, Instant};
    let now = Instant::now();

    // A track listing shows no progress, so nothing is re-read.
    let mut browser = Browser::default();
    browser.navigate(
        Target::Library {
            kind: Kind::Tracks,
            offset: 0,
            favorite: false,
        },
        "Tracks".into(),
    );
    browser.apply(browser.generation, Ok((vec![track()], None)));
    assert_eq!(browser.progress_changed(now), None);

    // A podcast listing does.
    let mut browser = Browser::default();
    browser.navigate(Target::InProgress, "Continue listening".into());
    browser.apply(browser.generation, Ok((vec![track()], None)));
    assert!(
        browser.progress_changed(now).is_some(),
        "a shelf built from progress re-reads"
    );
    browser.apply(browser.generation, Ok((vec![track()], None)));
    assert_eq!(
        browser.progress_changed(now + Duration::from_millis(500)),
        None,
        "a burst of events does not become a burst of requests"
    );
    browser.apply(browser.generation, Ok((vec![track()], None)));
    assert!(
        browser
            .progress_changed(now + Duration::from_secs(4))
            .is_some(),
        "but it does catch up once the throttle expires"
    );
}

/// A row says what the item belongs to, which is a different question per
/// media type — and never a provider instance id, which means nothing.
#[test]
fn a_row_names_the_show_the_album_or_the_author_but_never_a_provider_id() {
    let episode = Media::parse(
        &json!({"name":"Episode 12","item_id":"e1","provider":"audiobookshelf--zdGFJfeu",
                "media_type":"podcast_episode","uri":"library://podcast_episode/e1",
                "podcast":{"name":"The Cavan Sullivan Show"}}),
        "",
    );
    assert_eq!(episode.detail, "The Cavan Sullivan Show");

    let track = Media::parse(
        &json!({"name":"A Song","item_id":"t1","provider":"library","media_type":"track",
                "uri":"library://track/t1","artists":[{"name":"An Artist"}],
                "album":{"name":"An Album"}}),
        "",
    );
    assert_eq!(track.detail, "An Artist · An Album");

    // Audiobook authors may be plain strings rather than objects.
    let book = Media::parse(
        &json!({"name":"A Book","item_id":"b1","provider":"abs","media_type":"audiobook",
                "uri":"library://audiobook/b1","authors":["An Author"],
                "resume_position_ms":9_305_000}),
        "",
    );
    assert_eq!(book.detail, "An Author · resume 2:35:05");

    // With nothing to say, the media type is said readably rather than a
    // provider id being shown in its place.
    let bare = Media::parse(
        &json!({"name":"Something","item_id":"x","provider":"audiobookshelf--zdGFJfeu",
                "media_type":"podcast_episode","uri":"library://podcast_episode/x"}),
        "",
    );
    assert_eq!(bare.detail, "podcast episode");
    assert!(!bare.detail.contains("zdGFJfeu"));
}
