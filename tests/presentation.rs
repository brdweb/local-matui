use local_matui::{
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

#[test]
fn local_selection_resolves_universal_wrapper_and_preserves_user_selection() {
    use local_matui::presentation::select_local;
    let mut app = App {
        connected: true,
        players: vec![
            PlayerView {
                id: "remote".into(),
                available: true,
                name: "Local Matui".into(),
                ..Default::default()
            },
            PlayerView {
                id: "wrapper".into(),
                available: true,
                details: serde_json::json!({"output_protocols":[{"output_protocol_id":"local-sendspin"}]}),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    assert_eq!(
        select_local(&mut app, "local-sendspin"),
        Some("wrapper".into())
    );
    assert_eq!(app.player_cursor, 1);
    app.selected_id = Some("remote".into());
    assert_eq!(select_local(&mut app, "local-sendspin"), None);
    assert_eq!(app.selected_id.as_deref(), Some("remote"));
    app.selected_id = None;
    app.players[1].available = false;
    assert_eq!(select_local(&mut app, "local-sendspin"), None);
    app.players[1].id = "local-sendspin".into();
    app.players[1].available = true;
    app.players[1].details = serde_json::Value::Null;
    assert_eq!(
        select_local(&mut app, "local-sendspin"),
        Some("local-sendspin".into())
    );
}
