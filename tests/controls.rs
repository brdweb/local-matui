use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use matui::{
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
    assert!(menu.entries.iter().any(|(s, _)| s == "Unmute"));
    assert!(menu.entries.iter().any(|(s, _)| s == "Source: Aux"));
    assert!(!menu.entries.iter().any(|(s, _)| s == "Source: Passive"));
    assert!(menu
        .entries
        .iter()
        .any(|(s, _)| s == "Join group led by Compatible"));
    assert!(!menu
        .entries
        .iter()
        .any(|(s, _)| s == "Join group led by Other"));
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
