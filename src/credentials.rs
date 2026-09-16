//! Desktop Secret Service integration. Secrets travel through stdin/stdout pipes,
//! never command arguments, TOML, logging, or shell interpolation.
use anyhow::{bail, Context, Result};
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

/// Keyring attribute identifying this application's entries.
const APPLICATION: &str = "ma-tui";
/// Names used before each rename, newest first. Read, never written, so an
/// existing login keeps working without being re-entered.
const LEGACY_APPLICATIONS: &[&str] = &["local-matui", "matui"];

async fn invoke(application: &str, server: &str, id: &str, token: Option<&str>) -> Result<String> {
    let mut command = Command::new("secret-tool");
    if token.is_some() {
        command.args(["store", "--label=MA-TUI Music Assistant"]);
    } else {
        command.arg("lookup");
    }
    command
        .args(["application", application, "server", server, "player", id])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .context("Secret Service unavailable; install libsecret and unlock your keyring")?;
    let operation = async {
        if let Some(token) = token {
            let mut input = child.stdin.take().context("Cannot open keyring input")?;
            input
                .write_all(token.as_bytes())
                .await
                .context("Cannot send token to keyring")?;
        }
        let output = child
            .wait_with_output()
            .await
            .context("Keyring operation failed")?;
        if !output.status.success() {
            bail!("No saved login or keyring is locked; enter credentials in Settings");
        }
        String::from_utf8(output.stdout)
            .map(|v| v.trim().to_owned())
            .map_err(|_| anyhow::anyhow!("Invalid saved credential"))
    };
    tokio::time::timeout(Duration::from_secs(30), operation)
        .await
        .context("Keyring timed out; unlock the desktop keyring and retry")?
}

/// Look up the current entry, then a pre-rename one for the same server and
/// player. Report the current lookup's error so a locked or missing keyring is
/// still described accurately.
pub async fn load(server: &str, id: &str) -> Result<String> {
    let error = match invoke(APPLICATION, server, id, None).await {
        Ok(token) => return Ok(token),
        Err(error) => error,
    };
    for legacy in LEGACY_APPLICATIONS {
        if let Ok(token) = invoke(legacy, server, id, None).await {
            return Ok(token);
        }
    }
    Err(error)
}
pub async fn save(server: &str, id: &str, token: &str) -> Result<()> {
    invoke(APPLICATION, server, id, Some(token))
        .await
        .map(|_| ())
}
