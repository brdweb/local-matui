use local_matui::cli;

#[test]
fn offline_mode_cannot_accidentally_enable_audio() {
    use clap::Parser;
    assert!(cli::Args::try_parse_from(["local-matui", "--demo", "--local"]).is_err());
    assert!(cli::Args::try_parse_from(["local-matui", "--snapshot"]).is_err());
    assert!(cli::Args::try_parse_from(["local-matui", "--demo", "--snapshot"]).is_ok());
    assert!(cli::Args::try_parse_from(["local-matui", "--local", "--remote-only"]).is_err());
}
use local_matui::config;

#[test]
fn initializes_private_config_without_overwriting_identity() {
    let dir = std::env::temp_dir().join(format!("local-matui-test-{}", uuid::Uuid::new_v4()));
    let path = dir.join("config.toml");
    cli::initialize(&path).unwrap();
    let original = std::fs::read_to_string(&path).unwrap();
    let cfg = config::Config::parse(&original).unwrap();
    assert!(cfg.player_id.starts_with("local-matui-"));
    assert!(cli::initialize(&path).is_err());
    assert_eq!(original, std::fs::read_to_string(&path).unwrap());
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn configuration_written_before_the_rename_is_still_found() {
    let base = std::env::temp_dir().join(format!("local-matui-rename-{}", uuid::Uuid::new_v4()));
    let current = base.join("local-matui/config.toml");
    let legacy = base.join("matui/config.toml");
    // A fresh installation uses the current directory.
    assert_eq!(cli::config_in(&base), current);
    cli::initialize(&legacy).unwrap();
    assert_eq!(cli::config_in(&base), legacy);
    // Once a current configuration exists it wins; the legacy file is untouched.
    cli::initialize(&current).unwrap();
    assert_eq!(cli::config_in(&base), current);
    assert!(legacy.exists());
    // An explicit --config path overrides both.
    let explicit = base.join("elsewhere.toml");
    assert_eq!(cli::config_path(Some(explicit.clone())).unwrap(), explicit);
    std::fs::remove_dir_all(base).unwrap();
}
