//! Read Omarchy palettes without changing desktop settings or installing hooks.
use ratatui::style::Color;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub secondary: Color,
    pub selection: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            background: Color::Reset,
            foreground: Color::Reset,
            accent: Color::Cyan,
            secondary: Color::Gray,
            selection: Color::DarkGray,
        }
    }
}

impl Palette {
    pub fn parse(text: &str) -> Option<Self> {
        let values: toml::Value = toml::from_str(text).ok()?;
        let color = |keys: &[&str]| {
            keys.iter().find_map(|key| {
                let hex = values.get(key)?.as_str()?.trim_start_matches('#');
                if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return None;
                }
                let n = u32::from_str_radix(hex, 16).ok()?;
                Some(Color::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
            })
        };
        let foreground = color(&["foreground", "fg"])?;
        Some(Self {
            background: color(&["background", "bg"])?,
            foreground,
            accent: color(&["accent", "blue", "color4"]).unwrap_or(foreground),
            secondary: color(&["light_foreground", "bright_foreground", "foreground", "fg"])
                .unwrap_or(foreground),
            selection: color(&[
                "selection",
                "selection_background",
                "lighter_background",
                "color8",
            ])
            .unwrap_or(Color::DarkGray),
        })
    }
}

pub fn paths() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return vec![];
    };
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"));
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    vec![
        state.join("omarchy/current/theme/colors.toml"),
        home.join(".local/state/omarchy/current/theme/colors.toml"),
        config.join("omarchy/current/theme/colors.toml"),
    ]
}

/// Reopen the path each time: theme changes replace directories/symlinks.
/// Keep the last valid palette while Omarchy is staging the next theme.
pub fn reload(palette: &mut Palette, paths: &[PathBuf]) {
    for path in paths {
        if let Some(next) = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| Palette::parse(&s))
        {
            *palette = next;
            break;
        }
    }
}
