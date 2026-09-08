use crate::config::Config;
use anyhow::{Context, Result};
use std::{io::Write, os::unix::fs::OpenOptionsExt, path::Path};

#[derive(clap::Parser)]
#[command(
    version,
    about = "Music Assistant terminal controller and embedded Sendspin player"
)]
pub struct Args {
    /// Open connection/login and local speaker settings.
    #[arg(long, conflicts_with_all = ["demo", "init", "list_devices"])]
    pub setup: bool,
    /// Non-secret configuration file (default: $XDG_CONFIG_HOME/local-matui/config.toml).
    #[arg(long)]
    pub config: Option<std::path::PathBuf>,
    /// Create a new configuration with a persistent player ID; never overwrite.
    #[arg(long, conflicts_with_all = ["demo", "local", "list_devices"])]
    pub init: bool,
    /// Offline interface demonstration with explicitly fictional sample data.
    #[arg(long, conflicts_with_all = ["local", "list_devices"])]
    pub demo: bool,
    /// Print the offline demo as plain text rather than opening the terminal UI.
    #[arg(long, requires = "demo")]
    pub snapshot: bool,
    /// Enable local audio. Connects/registers this computer with Music Assistant.
    #[arg(long, conflicts_with = "remote_only")]
    pub local: bool,
    /// Disable local audio even when enabled in configuration.
    #[arg(long)]
    pub remote_only: bool,
    /// List local audio devices and exit without connecting to Music Assistant.
    #[arg(long)]
    pub list_devices: bool,
}

/// Configuration directory under the current name.
const DIRECTORY: &str = "local-matui";
/// Directory used before the rename; read, never created.
const LEGACY_DIRECTORY: &str = "matui";

/// Resolve the configuration file, honouring an explicit `--config` path.
pub fn config_path(explicit: Option<std::path::PathBuf>) -> Result<std::path::PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(home) => std::path::PathBuf::from(home),
        None => std::path::PathBuf::from(
            std::env::var_os("HOME")
                .ok_or_else(|| anyhow::anyhow!("Use --config when HOME is unset"))?,
        )
        .join(".config"),
    };
    Ok(config_in(&base))
}

/// A configuration written before the rename keeps working while no current one
/// exists. Nothing is copied or removed: moving the file switches directories.
pub fn config_in(base: &Path) -> std::path::PathBuf {
    let path = base.join(DIRECTORY).join("config.toml");
    let legacy = base.join(LEGACY_DIRECTORY).join("config.toml");
    if !path.exists() && legacy.exists() {
        legacy
    } else {
        path
    }
}

pub fn initialize(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).context("Cannot create configuration directory")?;
    }
    let text = toml::to_string_pretty(&Config::default())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .context("Cannot create configuration (it may already exist)")?;
    file.write_all(text.as_bytes())
        .context("Cannot write configuration")?;
    file.sync_all().context("Cannot persist configuration")?;
    Ok(())
}
