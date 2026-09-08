#[path = "../src/ui.rs"]
mod ui;

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
    for (code, action) in [
        (KeyCode::Char(' '), ui::Action::Toggle),
        (KeyCode::Char('n'), ui::Action::Next),
        (KeyCode::Char('p'), ui::Action::Previous),
        (KeyCode::Char('+'), ui::Action::Volume(5)),
        (KeyCode::Left, ui::Action::Seek(-10)),
    ] {
        assert_eq!(app.key(key(code)), action);
    }
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
    app.key(key(KeyCode::Esc));
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
            assert!(text.contains("MATUI"));
            assert!(text.contains("Disconnected"));
            assert!(text.contains("No player selected"));
        }
    }
}
