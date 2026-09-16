//! Opt-in desktop test using a disposable synthetic credential; removes it afterward.
#[tokio::test]
#[ignore = "requires an unlocked desktop Secret Service and secret-tool"]
async fn desktop_keyring_round_trip() {
    let id = format!("ma-tui-test-{}", uuid::Uuid::new_v4());
    let server = "https://ma-tui-fixture.invalid";
    let token = uuid::Uuid::new_v4().to_string();
    let saved = ma_tui::credentials::save(server, &id, &token).await;
    let loaded = ma_tui::credentials::load(server, &id).await;
    let cleanup = tokio::process::Command::new("secret-tool")
        .args([
            "clear",
            "application",
            "ma-tui",
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

/// Logins saved before the rename stay usable without re-entering credentials.
#[tokio::test]
#[ignore = "requires an unlocked desktop Secret Service and secret-tool"]
async fn reads_a_login_stored_under_the_former_application_name() {
    let id = format!("ma-tui-test-{}", uuid::Uuid::new_v4());
    let server = "https://ma-tui-fixture.invalid";
    let token = uuid::Uuid::new_v4().to_string();
    let attributes = ["application", "matui", "server", server, "player", &id];
    let mut store = tokio::process::Command::new("secret-tool")
        .args(["store", "--label=MA-TUI rename test"])
        .args(attributes)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use tokio::io::AsyncWriteExt;
        let mut input = store.stdin.take().unwrap();
        input.write_all(token.as_bytes()).await.unwrap();
    }
    assert!(store.wait().await.unwrap().success());
    let loaded = ma_tui::credentials::load(server, &id).await;
    let cleanup = tokio::process::Command::new("secret-tool")
        .arg("clear")
        .args(attributes)
        .status()
        .await
        .unwrap();
    assert!(cleanup.success());
    assert!(loaded.is_ok_and(|v| v == token));
}
