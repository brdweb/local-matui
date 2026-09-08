use std::process::Command;

#[test]
fn init_command_creates_configuration_without_token() {
    let dir = std::env::temp_dir().join(format!("local-matui-cli-{}", uuid::Uuid::new_v4()));
    let path = dir.join("config.toml");
    let result = Command::new(env!("CARGO_BIN_EXE_local-matui"))
        .arg("--init")
        .arg("--config")
        .arg(&path)
        .env_remove("LOCAL_MATUI_TOKEN")
        .env_remove("MATUI_TOKEN")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("player_id"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn offline_snapshot_is_real_rendering_and_explicitly_labeled() {
    let output = Command::new(env!("CARGO_BIN_EXE_local-matui"))
        .args(["--demo", "--snapshot"])
        .env_remove("LOCAL_MATUI_TOKEN")
        .env_remove("MATUI_TOKEN")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("OFFLINE DEMO"));
    assert!(text.contains("LOCAL-MATUI"));
    assert!(text.contains("Sample track"));
}
