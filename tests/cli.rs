#[path = "../src/cli.rs"]
mod cli;

#[test]
fn offline_mode_cannot_accidentally_enable_audio() {
    use clap::Parser;
    assert!(cli::Args::try_parse_from(["matui", "--demo", "--local"]).is_err());
    assert!(cli::Args::try_parse_from(["matui", "--snapshot"]).is_err());
    assert!(cli::Args::try_parse_from(["matui", "--demo", "--snapshot"]).is_ok());
    assert!(cli::Args::try_parse_from(["matui", "--local", "--remote-only"]).is_err());
}
#[path = "../src/config.rs"]
mod config;

#[test]
fn initializes_private_config_without_overwriting_identity() {
    let dir = std::env::temp_dir().join(format!("matui-test-{}", uuid::Uuid::new_v4()));
    let path = dir.join("config.toml");
    cli::initialize(&path).unwrap();
    let original = std::fs::read_to_string(&path).unwrap();
    let cfg = config::Config::parse(&original).unwrap();
    assert!(cfg.player_id.starts_with("matui-"));
    assert!(cli::initialize(&path).is_err());
    assert_eq!(original, std::fs::read_to_string(&path).unwrap());
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_dir_all(dir).unwrap();
}
