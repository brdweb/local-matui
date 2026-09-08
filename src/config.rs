use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Non-secret settings. Access tokens are read separately from MATUI_TOKEN.
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: "http://localhost:8095".into(),
            player_id: format!("matui-{}", uuid::Uuid::new_v4()),
            player_name: "Matui".into(),
            device_id: None,
            local_playback: false,
            volume: 30,
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
        let temp = parent.join(format!(".matui-{}.tmp", uuid::Uuid::new_v4()));
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
