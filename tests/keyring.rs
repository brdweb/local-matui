//! Opt-in desktop test using a disposable synthetic credential; removes it afterward.
#[tokio::test]
#[ignore = "requires an unlocked desktop Secret Service and secret-tool"]
async fn desktop_keyring_round_trip() {
    let id = format!("matui-test-{}", uuid::Uuid::new_v4());
    let server = "https://matui-fixture.invalid";
    let token = uuid::Uuid::new_v4().to_string();
    let saved = matui::credentials::save(server, &id, &token).await;
    let loaded = matui::credentials::load(server, &id).await;
    let cleanup = tokio::process::Command::new("secret-tool")
        .args([
            "clear",
            "application",
            "matui",
            "server",
            server,
            "player",
            &id,
        ])
        .status()
        .await
        .unwrap();
    assert!(cleanup.success());
    assert!(saved.is_ok());
    assert!(loaded.is_ok_and(|v| v == token));
}
