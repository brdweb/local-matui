use local_matui::ui;

#[test]
fn selects_available_player_and_routes_controls_only_when_connected() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let key = |c| KeyEvent::new(c, KeyModifiers::NONE);
    let mut app = ui::App::default();
    assert_eq!(app.key(key(KeyCode::Char(' '))), ui::Action::None);
    app.connected = true;
    app.players = vec![
        ui::PlayerView {
            id: "one".into(),
            available: false,
            ..Default::default()
        },
        ui::PlayerView {
            id: "two".into(),
            available: true,
            ..Default::default()
        },
    ];
    assert_eq!(app.key(key(KeyCode::Enter)), ui::Action::None);
    app.key(key(KeyCode::Down));
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        ui::Action::Select("two".into())
    );
    assert_eq!(app.selected_id.as_deref(), Some("two"));
    let volume = |name| {
        ui::Action::Command(local_matui::controls::Command::Player {
            name,
            args: serde_json::json!({}),
        })
    };
    for (code, action) in [
        // p is play/pause, as in other players; tracks move to < and >.
        (KeyCode::Char(' '), ui::Action::Toggle),
        (KeyCode::Char('p'), ui::Action::Toggle),
        (KeyCode::Char('n'), ui::Action::Next),
        (KeyCode::Char('>'), ui::Action::Next),
        (KeyCode::Char('<'), ui::Action::Previous),
        (KeyCode::Char(','), ui::Action::Previous),
        // Volume steps are server commands, not a read-modify-write.
        (KeyCode::Char('+'), volume("volume_up")),
        (KeyCode::Char('-'), volume("volume_down")),
    ] {
        assert_eq!(app.key(key(code)), action, "{code:?}");
    }
    // Seeking resolves the target from what is on screen.
    assert_eq!(app.key(key(KeyCode::Left)), ui::Action::None);
    assert!(app.status.contains("no seekable duration"));
    app.duration = 100.0;
    app.elapsed = 50.0;
    assert_eq!(app.key(key(KeyCode::Left)), ui::Action::Seek(40.0));
    assert_eq!(app.key(key(KeyCode::Left)), ui::Action::Seek(30.0));
    assert_eq!(app.key(key(KeyCode::Right)), ui::Action::Seek(40.0));
    assert_eq!(app.elapsed, 40.0, "the position on screen follows the seek");
    app.elapsed = 0.0;
    assert_eq!(app.key(key(KeyCode::Left)), ui::Action::Seek(0.0));
    app.focus = ui::Focus::Search;
    app.results = vec![ui::TrackView {
        uri: "library://track/1".into(),
        ..Default::default()
    }];
    assert_eq!(
        app.key(key(KeyCode::Char('a'))),
        ui::Action::Enqueue("library://track/1".into())
    );
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        ui::Action::Play("library://track/1".into())
    );
    // Esc leaves search for the browser rather than jumping to the queue.
    app.key(key(KeyCode::Esc));
    assert!(app.focus == ui::Focus::Music);
    app.key(key(KeyCode::F(4)));
    assert!(app.focus == ui::Focus::Queue);
    assert_eq!(app.key(key(KeyCode::Char('r'))), ui::Action::Refresh);
}

#[test]
fn search_typing_never_triggers_transport_or_quit() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let key = |c| KeyEvent::new(c, KeyModifiers::NONE);
    let mut app = ui::App::default();
    assert_eq!(app.key(key(KeyCode::Char('/'))), ui::Action::None);
    for c in "quiet night".chars() {
        assert_eq!(app.key(key(KeyCode::Char(c))), ui::Action::None);
    }
    assert_eq!(
        app.key(key(KeyCode::Enter)),
        ui::Action::Search("quiet night".into())
    );
    assert_eq!(app.key(key(KeyCode::Char('q'))), ui::Action::Quit);
}

#[test]
fn renders_disconnected_and_small_terminal_without_panicking() {
    for (width, height) in [(110, 32), (30, 8), (1, 1)] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        let mut app = ui::App::default();
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        if width > 50 {
            assert!(text.contains("LOCAL-MATUI"));
            assert!(text.contains("Disconnected"));
            assert!(text.contains("No player selected"));
        }
    }
}

/// The visualizer key only opens a view that can show something real.
#[test]
fn the_visualizer_opens_only_with_local_audio_and_closes_with_esc() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use local_matui::visualizer::Mode;
    let key = |c| KeyEvent::new(c, KeyModifiers::NONE);

    let mut remote = ui::App::default();
    assert_eq!(remote.key(key(KeyCode::Char('v'))), ui::Action::None);
    assert_eq!(
        remote.visualizer.mode,
        Mode::Off,
        "without Local Matui's own speaker there is nothing to visualize"
    );
    assert!(remote.status.contains("local audio"));

    let mut app = ui::App {
        spectrum: Some(local_matui::visualizer::Analyzer::new()),
        ..Default::default()
    };
    app.key(key(KeyCode::Char('v')));
    assert_eq!(app.visualizer.mode, Mode::Panel);
    app.key(key(KeyCode::Char('v')));
    assert_eq!(app.visualizer.mode, Mode::Full);
    // Esc closes the visualizer before it means anything else, and never
    // doubles as a pane switch.
    let focus = app.focus;
    app.key(key(KeyCode::Esc));
    assert_eq!(app.visualizer.mode, Mode::Off);
    assert!(app.focus == focus, "Esc must not also switch panes");
    app.key(key(KeyCode::Esc));
    assert!(app.focus == focus);
}

#[test]
fn both_visualizer_views_render_and_explain_a_silent_endpoint() {
    use local_matui::visualizer::Mode;
    let mut app = ui::App {
        spectrum: Some(local_matui::visualizer::Analyzer::new()),
        connected: true,
        selected_id: Some("kitchen".into()),
        local_endpoint: Some("local-matui-endpoint".into()),
        players: vec![ui::PlayerView {
            id: "kitchen".into(),
            name: "Kitchen".into(),
            available: true,
            state: "playing".into(),
            ..Default::default()
        }],
        title: "Something".into(),
        ..Default::default()
    };
    for mode in [Mode::Panel, Mode::Full] {
        app.visualizer.mode = mode;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(110, 30)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            text.contains("no local audio · playing on Kitchen"),
            "a remote speaker must be named as the reason, in {mode:?}"
        );
        assert!(
            !text.contains('█'),
            "no bars without local samples in {mode:?}"
        );
    }
}

/// Shuffle and repeat are one key each, and only where the server reports them.
#[test]
fn shuffle_and_repeat_keys_act_on_the_displayed_queue() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use local_matui::controls::Command;
    use serde_json::json;
    let key = |c| KeyEvent::new(c, KeyModifiers::NONE);
    let mut app = ui::App {
        connected: true,
        selected_id: Some("one".into()),
        players: vec![ui::PlayerView {
            id: "one".into(),
            available: true,
            ..Default::default()
        }],
        focus: ui::Focus::Queue,
        ..Default::default()
    };
    // No queue yet: the key explains instead of guessing an identity.
    assert_eq!(app.key(key(KeyCode::Char('z'))), ui::Action::None);
    assert!(app.status.contains("No active queue"));
    // Stop is a player command and needs no queue identity.
    assert_eq!(
        app.key(key(KeyCode::Char('s'))),
        ui::Action::Command(Command::Player {
            name: "stop",
            args: json!({})
        })
    );

    app.queue_id = "leader".into();
    app.queue_details = json!({"shuffle_enabled": true, "repeat_mode": "all"});
    assert_eq!(
        app.key(key(KeyCode::Char('z'))),
        ui::Action::Command(Command::Queue {
            id: "leader".into(),
            name: "shuffle",
            args: json!({"shuffle_enabled": false})
        })
    );
    // Repeat cycles off → all → one → off.
    for (mode, next) in [("off", "all"), ("all", "one"), ("one", "off")] {
        app.queue_details = json!({ "repeat_mode": mode });
        assert_eq!(
            app.key(key(KeyCode::Char('l'))),
            ui::Action::Command(Command::Queue {
                id: "leader".into(),
                name: "repeat",
                args: json!({ "repeat_mode": next })
            })
        );
    }
    app.queue_details = json!({"is_dynamic": true});
    assert_eq!(app.key(key(KeyCode::Char('z'))), ui::Action::None);
    assert!(app.status.contains("dynamic queue"));
}

/// The queue stays on screen while browsing, and the header carries state.
#[test]
fn the_queue_and_browser_are_visible_together_with_playback_state() {
    let mut app = ui::App {
        connected: true,
        selected_id: Some("one".into()),
        queue_id: "leader".into(),
        queue_details: serde_json::json!({"shuffle_enabled": true, "repeat_mode": "one"}),
        players: vec![ui::PlayerView {
            id: "one".into(),
            name: "Kitchen".into(),
            available: true,
            state: "playing".into(),
            volume: Some(42),
            details: serde_json::json!({"volume_muted": true}),
        }],
        queue: vec![ui::TrackView {
            id: "q1".into(),
            title: "Queued track".into(),
            ..Default::default()
        }],
        title: "Now playing this".into(),
        ..Default::default()
    };
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(110, 30)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    for expected in [
        "PLAYERS",
        "QUEUE · 1 item",
        "Queued track",
        "MUSIC",
        "Now playing this",
        "playing",
        "vol 42%",
        "muted",
        "shuffle on",
        "repeat one",
    ] {
        assert!(
            text.contains(expected),
            "{expected} missing from the screen"
        );
    }
}
