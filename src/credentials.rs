//! Desktop Secret Service integration. Secrets travel through stdin/stdout pipes,
//! never command arguments, TOML, logging, or shell interpolation.
use anyhow::{bail, Context, Result};
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

async fn invoke(server: &str, id: &str, token: Option<&str>) -> Result<String> {
    let mut command = Command::new("secret-tool");
    if token.is_some() {
        command.args(["store", "--label=Matui Music Assistant"]);
    } else {
        command.arg("lookup");
    }
    command
        .args(["application", "matui", "server", server, "player", id])
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

pub async fn load(server: &str, id: &str) -> Result<String> {
    invoke(server, id, None).await
}
pub async fn save(server: &str, id: &str, token: &str) -> Result<()> {
    invoke(server, id, Some(token)).await.map(|_| ())
}
