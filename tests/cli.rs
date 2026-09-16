use ma_tui::cli;

#[test]
fn offline_mode_cannot_accidentally_enable_audio() {
    use clap::Parser;
    assert!(cli::Args::try_parse_from(["ma-tui", "--demo", "--local"]).is_err());
    assert!(cli::Args::try_parse_from(["ma-tui", "--snapshot"]).is_err());
    assert!(cli::Args::try_parse_from(["ma-tui", "--demo", "--snapshot"]).is_ok());
    assert!(cli::Args::try_parse_from(["ma-tui", "--local", "--remote-only"]).is_err());
}
use ma_tui::config;

#[test]
fn initializes_private_config_without_overwriting_identity() {
    let dir = std::env::temp_dir().join(format!("ma-tui-test-{}", uuid::Uuid::new_v4()));
    let path = dir.join("config.toml");
    cli::initialize(&path).unwrap();
    let original = std::fs::read_to_string(&path).unwrap();
    let cfg = config::Config::parse(&original).unwrap();
    assert!(cfg.player_id.starts_with("ma-tui-"));
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
fn configuration_written_before_either_rename_is_still_found() {
    let base = std::env::temp_dir().join(format!("ma-tui-rename-{}", uuid::Uuid::new_v4()));
    let current = base.join("ma-tui/config.toml");
    let previous = base.join("local-matui/config.toml");
    let oldest = base.join("matui/config.toml");

    // A fresh installation uses the current directory.
    assert_eq!(cli::config_in(&base), current);

    // The oldest name is still found on its own.
    cli::initialize(&oldest).unwrap();
    assert_eq!(cli::config_in(&base), oldest);

    // With both earlier names present, the newer of them wins.
    cli::initialize(&previous).unwrap();
    assert_eq!(cli::config_in(&base), previous);

    // Once a current configuration exists it wins, and nothing earlier is
    // touched: the rename never moves or deletes a file.
    cli::initialize(&current).unwrap();
    assert_eq!(cli::config_in(&base), current);
    assert!(previous.exists() && oldest.exists());

    // An explicit --config path overrides all of them.
    let explicit = base.join("elsewhere.toml");
    assert_eq!(cli::config_path(Some(explicit.clone())).unwrap(), explicit);
    std::fs::remove_dir_all(base).unwrap();
}
