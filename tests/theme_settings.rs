use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use matui::{
    config::Config,
    settings::Settings,
    theme::{self, Palette},
    ui::{Action, App},
};
use ratatui::style::Color;

#[test]
fn palette_tracks_replaced_directory_and_keeps_last_valid_colors() {
    let dir = std::env::temp_dir().join(format!("matui-theme-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("colors.toml");
    std::fs::write(
        &path,
        "background='#010203'\nforeground='#eeeeee'\naccent='#ff0000'",
    )
    .unwrap();
    let mut palette = Palette::default();
    theme::reload(&mut palette, std::slice::from_ref(&path));
    assert_eq!(palette.background, Color::Rgb(1, 2, 3));
    let replacement = dir.join("next.toml");
    std::fs::write(
        &replacement,
        "background='#ffffff'\nforeground='#111111'\naccent='#0000ff'",
    )
    .unwrap();
    std::fs::rename(replacement, &path).unwrap();
    theme::reload(&mut palette, std::slice::from_ref(&path));
    assert_eq!(palette.background, Color::Rgb(255, 255, 255));
    assert_eq!(palette.accent, Color::Rgb(0, 0, 255));
    let last = palette;
    std::fs::write(&path, "background = 'incomplete").unwrap();
    theme::reload(&mut palette, std::slice::from_ref(&path));
    assert_eq!(palette, last);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn settings_masks_credentials_and_never_dispatches_playback_shortcuts() {
    let mut app = App {
        settings: Some(Settings::new(Config::default())),
        ..Default::default()
    };
    app.settings.as_mut().unwrap().field = 2;
    for c in "quiet secret +np".chars() {
        assert_eq!(
            app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
            Action::None
        );
    }
    for size in [(110, 30), (50, 16), (1, 1)] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(size.0, size.1)).unwrap();
        terminal.draw(|f| matui::ui::draw(f, &mut app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(!text.contains("quiet secret"));
    }
    assert_eq!(app.settings.as_ref().unwrap().password, "quiet secret +np");
    app.settings.as_mut().unwrap().field = 7;
    assert_eq!(
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Action::SaveSettings
    );
}

#[test]
fn saving_settings_preserves_identity_and_private_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("matui-settings-{}", uuid::Uuid::new_v4()));
    let path = dir.join("config.toml");
    let mut config = Config::default();
    config.save(&path).unwrap();
    config.player_name = "Laptop".into();
    config.save(&path).unwrap();
    let saved = Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(config.player_id, saved.player_id);
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(saved.player_name, "Laptop");
    std::fs::remove_dir_all(dir).unwrap();
}
