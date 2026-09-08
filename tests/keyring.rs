//! Opt-in desktop test using a disposable synthetic credential; removes it afterward.
#[tokio::test]
#[ignore = "requires an unlocked desktop Secret Service and secret-tool"]
async fn desktop_keyring_round_trip() {
    let id = format!("local-matui-test-{}", uuid::Uuid::new_v4());
    let server = "https://local-matui-fixture.invalid";
    let token = uuid::Uuid::new_v4().to_string();
    let saved = local_matui::credentials::save(server, &id, &token).await;
    let loaded = local_matui::credentials::load(server, &id).await;
    let cleanup = tokio::process::Command::new("secret-tool")
        .args([
            "clear",
            "application",
            "local-matui",
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
    let id = format!("local-matui-test-{}", uuid::Uuid::new_v4());
    let server = "https://local-matui-fixture.invalid";
    let token = uuid::Uuid::new_v4().to_string();
    let attributes = ["application", "matui", "server", server, "player", &id];
    let mut store = tokio::process::Command::new("secret-tool")
        .args(["store", "--label=Local Matui rename test"])
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
    let loaded = local_matui::credentials::load(server, &id).await;
    let cleanup = tokio::process::Command::new("secret-tool")
        .arg("clear")
        .args(attributes)
        .status()
        .await
        .unwrap();
    assert!(cleanup.success());
    assert!(loaded.is_ok_and(|v| v == token));
}
