#[path = "../src/config.rs"]
mod config;

#[test]
fn parses_server_without_storing_a_token() {
    let cfg =
        config::Config::parse("server = 'https://ma.example:8095'\nplayer_id = 'matui-test'\n")
            .unwrap();
    assert_eq!(cfg.server, "https://ma.example:8095");
    assert_eq!(cfg.player_id, "matui-test");
    assert!(!cfg.local_playback);
    assert_eq!(cfg.volume, 30);
}

#[test]
fn rejects_unsafe_server_urls_and_empty_identity() {
    for server in [
        "ftp://host",
        "http://user:password@host",
        "http://host/?token=secret",
        "http://host/#secret",
    ] {
        assert!(
            config::Config::parse(&format!("server = '{server}'\nplayer_id = 'test'\n")).is_err()
        );
    }
    assert!(config::Config::parse("player_id = ''").is_err());
}

#[test]
fn loading_a_config_must_not_generate_a_new_player_identity() {
    assert!(config::Config::parse("server = 'http://localhost:8095'").is_err());
}
