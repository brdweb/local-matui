use matui::{
    api::{Player, Queue},
    controller::Update,
    presentation::apply,
    ui::{App, PlayerView},
};

#[test]
fn stale_queue_cannot_overwrite_new_player_and_highlight_survives_reorder() {
    let mut app = App {
        selected_id: Some("new".into()),
        title: "New player".into(),
        players: vec![PlayerView {
            id: "highlighted".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    apply(
        &mut app,
        Update::Queue(
            "old".into(),
            Ok(Queue {
                current_title: "Old track".into(),
                ..Default::default()
            }),
        ),
    );
    assert_eq!(app.title, "New player");
    apply(
        &mut app,
        Update::Players(vec![
            Player {
                id: "other".into(),
                ..Default::default()
            },
            Player {
                id: "highlighted".into(),
                ..Default::default()
            },
        ]),
    );
    assert_eq!(app.player_cursor, 1);
    apply(&mut app, Update::Offline("Connection failed".into()));
    assert!(!app.connected);
    assert!(app.status.contains("stale"));
}

#[test]
fn metadata_is_not_allowed_to_emit_terminal_control_characters() {
    let mut app = App::default();
    apply(
        &mut app,
        Update::Players(vec![Player {
            id: "id".into(),
            name: "bad\x1b]52;payload\x07\nname".into(),
            ..Default::default()
        }]),
    );
    assert!(!app.players[0].name.chars().any(char::is_control));
    assert_eq!(app.players[0].id, "id");
}
