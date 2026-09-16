use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// How the spectrum is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Spectrum {
    /// Braille cells carry 2x4 dots each, so the bars resolve four times finer
    /// than the character grid allows. Needs a font covering U+2800-28FF, which
    /// every Nerd Font and most monospace fonts do.
    #[default]
    Braille,
    /// Eighth-height blocks: coarser, but drawn with characters every font has.
    Blocks,
}

/// How album art is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AlbumArt {
    /// Sixel where the terminal is known to draw it and reports a pixel size,
    /// half blocks otherwise. This is a guess about the terminal, not a
    /// negotiation with it, so `sixel` and `blocks` override it.
    #[default]
    Auto,
    /// Real pixels, for a terminal that speaks sixel.
    Sixel,
    /// Two pixels per cell, drawn as ordinary styled cells. Works anywhere.
    Blocks,
    Off,
}

impl AlbumArt {
    /// Whether any cover is wanted at all.
    pub fn enabled(self) -> bool {
        self != Self::Off
    }
}

/// Non-secret settings. Access tokens are read separately from LOCAL_MATUI_TOKEN.
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: String,
    #[serde(default)]
    pub player_id: String,
    pub player_name: String,
    pub device_id: Option<String>,
    pub local_playback: bool,
    pub volume: u8,
    /// Switch to `blocks` if the terminal font has no braille glyphs.
    pub spectrum: Spectrum,
    /// Show album art, where the terminal and the server can both supply it.
    pub album_art: AlbumArt,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: "http://localhost:8095".into(),
            player_id: format!("local-matui-{}", uuid::Uuid::new_v4()),
            player_name: "Local Matui".into(),
            device_id: None,
            local_playback: false,
            volume: 30,
            spectrum: Spectrum::default(),
            album_art: AlbumArt::default(),
        }
    }
}

impl Config {
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let text = toml::to_string_pretty(self)?;
        Self::parse(&text)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        std::fs::create_dir_all(parent)?;
        let temp = parent.join(format!(".local-matui-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temp, path)?;
            std::fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result.map_err(|_| anyhow::anyhow!("Could not save connection settings"))
    }

    pub fn parse(text: &str) -> Result<Self> {
        // Do not echo TOML input: users may accidentally paste a credential.
        let value: Self = toml::from_str(text)
            .map_err(|_| anyhow::anyhow!("Invalid configuration TOML or unknown setting"))?;
        if value.volume > 100 {
            bail!("Volume must be between 0 and 100");
        }
        let url =
            url::Url::parse(&value.server).map_err(|_| anyhow::anyhow!("Invalid server URL"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("Server must be an HTTP(S) base URL without credentials, query, or fragment");
        }
        if value.player_id.trim().is_empty() || value.player_name.trim().is_empty() {
            bail!("Player identity and name cannot be empty");
        }
        Ok(value)
    }
}
