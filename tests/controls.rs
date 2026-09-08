use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use local_matui::{
    controls::{Command, InputKind, Menu, Prompt},
    ui::{Action, App, Focus, PlayerView, TrackView},
};
use serde_json::json;

#[test]
fn queue_shortcuts_use_item_identity_and_menu_respects_player_capabilities() {
    let mut app = App {
        connected: true,
        selected_id: Some("member".into()),
        queue_id: "leader".into(),
        focus: Focus::Queue,
        queue: vec![TrackView {
            id: "item-id".into(),
            ..Default::default()
        }],
        players: vec![
            PlayerView {
                id: "member".into(),
                available: true,
                details: json!({
                    "volume_muted":true,"powered":true,"can_group_with":["compatible"],
                    "source_list":[{"id":"aux","name":"Aux"},{"id":"passive","name":"Passive","passive":true}]
                }),
                ..Default::default()
            },
            PlayerView {
                id: "compatible".into(),
                available: true,
                name: "Compatible".into(),
                ..Default::default()
            },
            PlayerView {
                id: "other".into(),
                available: true,
                name: "Other".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    assert_eq!(
        app.key(key(KeyCode::Delete)),
        Action::Command(Command::Queue {
            id: "leader".into(),
            name: "delete_item",
            args: json!({"item_id_or_index":"item-id"})
        })
    );
    let menu = Menu::new(&app);
    let labelled = |text: &str| menu.entries.iter().any(|entry| entry.label == text);
    assert!(labelled("Unmute"));
    assert!(labelled("Source: Aux"));
    assert!(!labelled("Source: Passive"));
    assert!(labelled("Join group led by Compatible"));
    assert!(!labelled("Join group led by Other"));
    // Entries are grouped under headings, in the order the headings are shown.
    let sections: Vec<&str> = menu.entries.iter().map(|entry| entry.section).collect();
    let mut seen: Vec<&str> = vec![];
    for section in sections {
        if seen.last() != Some(&section) {
            assert!(!seen.contains(&section), "{section} appears twice");
            seen.push(section);
        }
    }
    assert_eq!(seen.first(), Some(&"Playback"));
    app.menu = Some(menu);
    app.connected = false;
    assert_eq!(app.key(key(KeyCode::Enter)), Action::None);
    assert!(app.menu.is_none());
}

#[test]
fn numeric_prompts_reject_nan_out_of_range_and_fractional_volume() {
    let mut prompt = Prompt {
        label: "Volume".into(),
        command: Command::Player {
            name: "volume_set",
            args: json!({}),
        },
        argument: "volume_level",
        value: String::new(),
        kind: InputKind::Number {
            min: 0.0,
            max: 100.0,
            integer: true,
        },
    };
    for invalid in ["nan", "-1", "101", "12.5", "", "inf"] {
        prompt.value = invalid.into();
        assert!(prompt.submit().is_err());
    }
    prompt.value = "42".into();
    assert_eq!(
        prompt.submit().unwrap(),
        Command::Player {
            name: "volume_set",
            args: json!({"volume_level":42})
        }
    );
}

/// A long menu is grouped and searchable; filtering never runs an action.
#[test]
fn the_controls_menu_filters_without_triggering_anything() {
    use local_matui::ui::Focus;
    let mut app = App {
        connected: true,
        selected_id: Some("one".into()),
        queue_id: "leader".into(),
        focus: Focus::Queue,
        players: vec![PlayerView {
            id: "one".into(),
            name: "Kitchen".into(),
            available: true,
            details: json!({"volume_muted": false, "powered": true}),
            ..Default::default()
        }],
        ..Default::default()
    };
    app.menu = Some(Menu::new(&app));
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    let total = app.menu.as_ref().unwrap().entries.len();
    assert!(total > 20, "this menu is long enough to need filtering");

    // Typing before "/" still navigates, as it always did.
    app.key(key(KeyCode::Char('j')));
    assert_eq!(app.menu.as_ref().unwrap().cursor, 1);

    app.key(key(KeyCode::Char('/')));
    for c in "sleep".chars() {
        assert_eq!(app.key(key(KeyCode::Char(c))), Action::None);
    }
    let menu = app.menu.as_ref().unwrap();
    assert!(menu.filtering);
    assert_eq!(menu.cursor, 0, "filtering starts from the first match");
    let shown = menu.visible();
    assert!(shown.len() < total && !shown.is_empty());
    assert!(shown.iter().all(
        |index| menu.entries[*index].label.to_lowercase().contains("sleep")
            || menu.entries[*index]
                .section
                .to_lowercase()
                .contains("sleep")
    ));
    // q would close the menu outside the filter; here it is just text.
    app.key(key(KeyCode::Char('q')));
    assert!(app.menu.is_some());
    assert_eq!(app.menu.as_ref().unwrap().visible().len(), 0);
    assert_eq!(app.key(key(KeyCode::Enter)), Action::None);
    assert!(app.menu.is_some(), "an empty filter has nothing to apply");

    app.key(key(KeyCode::Backspace));
    // Enter applies the entry the cursor is on, not the one at that index in
    // the unfiltered menu.
    let expected = app
        .menu
        .as_ref()
        .unwrap()
        .selected()
        .unwrap()
        .action
        .clone();
    assert!(
        matches!(&expected, Action::Command(Command::Player { name, .. }) if *name == "sleep_timer/set")
    );
    assert_eq!(app.key(key(KeyCode::Enter)), expected);
    assert!(app.menu.is_none());
}

/// Prints the controls menu: `cargo test --test controls -- --ignored --nocapture preview`.
#[test]
#[ignore = "prints a picture for inspection rather than asserting"]
fn preview() {
    let mut app = App {
        connected: true,
        selected_id: Some("one".into()),
        queue_id: "leader".into(),
        queue_details: json!({"shuffle_enabled": false, "repeat_mode": "off", "autoplay_enabled": true}),
        players: vec![
            PlayerView {
                id: "one".into(),
                name: "Kitchen".into(),
                available: true,
                details: json!({
                    "volume_muted": false, "powered": true, "can_group_with": ["two"],
                    "source_list": [{"id":"aux","name":"Aux"}, {"id":"tv","name":"TV"}]
                }),
                ..Default::default()
            },
            PlayerView {
                id: "two".into(),
                name: "Study".into(),
                available: true,
                ..Default::default()
            },
        ],
        queue: vec![TrackView {
            id: "q1".into(),
            title: "Queued track".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    app.menu = Some(Menu::new(&app));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(90, 26)).unwrap();
    terminal
        .draw(|frame| local_matui::ui::draw(frame, &mut app))
        .unwrap();
    for row in terminal.backend().buffer().content.chunks(90) {
        println!("{}", row.iter().map(|c| c.symbol()).collect::<String>());
    }
}
