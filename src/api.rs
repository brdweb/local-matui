//! Music Assistant 2.10.2 HTTP API (bare JSON results, not WS result envelopes).
//! Verified against server tag 2.10.2 controllers/webserver/controller.py and
//! the official music-assistant/client player_queues.py command signatures.
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct Player {
    pub details: Value,
    pub id: String,
    pub name: String,
    pub state: String,
    pub volume: Option<u8>,
    pub available: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Queue {
    pub details: Value,
    pub id: String,
    pub name: String,
    pub state: String,
    pub current_title: String,
    pub current_artist: String,
    pub elapsed: f64,
    pub duration: f64,
    pub items: Vec<QueueItem>,
}
#[derive(Debug, Clone, Default)]
pub struct QueueItem {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
}
#[derive(Debug, Clone, Default)]
pub struct Track {
    pub uri: String,
    pub title: String,
    pub artist: String,
}
#[derive(Debug, Clone)]
pub enum Control {
    Toggle,
    Next,
    Previous,
    Volume(u8),
    Seek(f64),
}
#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    endpoint: url::Url,
    token: String,
}
impl std::fmt::Debug for ApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiClient").finish_non_exhaustive()
    }
}
impl ApiClient {
    /// Built-in MA login. Use a profile token for Home Assistant/OAuth accounts.
    pub async fn login(server: &str, username: &str, password: &str) -> Result<String> {
        let api = Self::new(server, "login")?;
        api.server_version().await?;
        let mut endpoint = api.endpoint.clone();
        endpoint.set_path(&format!(
            "{}/auth/login",
            endpoint.path().strip_suffix("/api").unwrap_or_default()
        ));
        let response = api
            .http
            .post(endpoint)
            .json(&json!({
                "provider_id":"builtin", "device_name":"Matui",
                "credentials":{"username":username,"password":password}
            }))
            .send()
            .await
            .map_err(|_| anyhow!("Login connection failed"))?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "Login rejected (HTTP {})",
                response.status().as_u16()
            ));
        }
        let value: Value = response
            .json()
            .await
            .map_err(|_| anyhow!("Invalid login response"))?;
        if value["success"] != true {
            return Err(anyhow!("Login rejected"));
        }
        let token = value["token"]
            .as_str()
            .ok_or_else(|| anyhow!("Login did not return a token"))?;
        Self::new(server, token)?;
        Ok(token.to_owned())
    }

    pub async fn verify(&self) -> Result<String> {
        // Identify the base URL before sending credentials to its API endpoint.
        let version = self.server_version().await?;
        self.command("auth/me", json!({})).await?;
        self.players().await?;
        Ok(version)
    }

    async fn server_version(&self) -> Result<String> {
        let mut endpoint = self.endpoint.clone();
        endpoint.set_path(&format!(
            "{}/info",
            endpoint.path().strip_suffix("/api").unwrap_or_default()
        ));
        let response = self.http.get(endpoint).send().await.map_err(|_| {
            anyhow!("Cannot reach Music Assistant server information; check the server URL")
        })?;
        if !response.status().is_success() {
            return Err(anyhow!("Server information returned HTTP {}; use the direct Music Assistant base URL, not a Home Assistant dashboard or ingress URL", response.status().as_u16()));
        }
        let value: Value = response
            .json()
            .await
            .map_err(|_| anyhow!("This URL did not return Music Assistant server information; use the direct Music Assistant base URL"))?;
        let version = text(&value, "server_version");
        if version.is_empty() {
            return Err(anyhow!(
                "This URL did not report a Music Assistant server version; check the base URL"
            ));
        }
        Ok(version
            .chars()
            .filter(|c| !c.is_control())
            .take(60)
            .collect())
    }

    pub fn new(server: &str, token: &str) -> Result<Self> {
        let mut endpoint = url::Url::parse(server).map_err(|_| anyhow!("Invalid server URL"))?;
        // Url normalizes empty userinfo away, so reject its raw authority too.
        let raw_authority = server
            .split_once("://")
            .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default());
        if raw_authority.is_none_or(|authority| authority.contains('@'))
            || !matches!(endpoint.scheme(), "http" | "https")
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(anyhow!(
                "Server URL must be HTTP(S), without credentials, query or fragment"
            ));
        }
        if token.is_empty() || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(anyhow!("Invalid access token"));
        }
        endpoint.set_path(&format!("{}/api", endpoint.path().trim_end_matches('/')));
        Ok(Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(std::time::Duration::from_secs(3))
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|_| anyhow!("Unable to initialize HTTP client"))?,
            endpoint,
            token: token.into(),
        })
    }
    async fn command(&self, command: &str, args: Value) -> Result<Value> {
        let response = self
            .http
            .post(self.endpoint.clone())
            .bearer_auth(&self.token)
            .json(&json!({"message_id":"matui", "command":command,"args":args}))
            .send()
            .await
            .map_err(|_| anyhow!("Music Assistant connection failed"))?;
        if !response.status().is_success() {
            if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
                return Err(anyhow!("Music Assistant API rejected POST /api (HTTP 405); check the server base URL and reverse-proxy routing"));
            }
            return Err(anyhow!(
                "Music Assistant HTTP {}",
                response.status().as_u16()
            ));
        }
        response
            .json()
            .await
            .map_err(|_| anyhow!("Invalid Music Assistant response"))
    }
    async fn active_queue(&self, player_id: &str) -> Result<Value> {
        let v = self
            .command(
                "player_queues/get_active_queue",
                json!({"player_id":player_id}),
            )
            .await?;
        if text(&v, "queue_id").is_empty() {
            return Err(anyhow!("Player has no active queue"));
        }
        Ok(v)
    }
    pub async fn playback_command(
        &self,
        player_id: &str,
        command: crate::controls::Command,
    ) -> Result<()> {
        use crate::controls::Command;
        match command {
            Command::Player { name, mut args } => {
                let path = match name {
                    "sleep_timer/set" | "sleep_timer/clear" => format!("players/{name}"),
                    "set_members" => {
                        args["target_player"] = json!(player_id);
                        self.command("players/cmd/set_members", args).await?;
                        return Ok(());
                    }
                    _ => format!("players/cmd/{name}"),
                };
                args["player_id"] = json!(player_id);
                self.command(&path, args).await?;
            }
            Command::Queue { id, name, mut args } => {
                self.check_queue(player_id, &id).await?;
                args["queue_id"] = json!(id);
                self.command(&format!("player_queues/{name}"), args).await?;
            }
            Command::Transfer { source, target } => {
                self.check_queue(player_id, &source).await?;
                let destination = self.active_queue(&target).await?;
                let target_id = text(&destination, "queue_id");
                if source == target_id {
                    return Err(anyhow!("Players already share this queue"));
                }
                self.command(
                    "player_queues/transfer",
                    json!({"source_queue_id":source,"target_queue_id":target_id}),
                )
                .await?;
            }
        }
        Ok(())
    }
    async fn check_queue(&self, player_id: &str, expected: &str) -> Result<()> {
        let active = self.active_queue(player_id).await?;
        if expected.is_empty() || text(&active, "queue_id") != expected {
            return Err(anyhow!("Active queue changed; refresh before editing it"));
        }
        Ok(())
    }
    pub async fn queue(&self, player_id: &str) -> Result<Queue> {
        let v = self.active_queue(player_id).await?;
        let id = text(&v, "queue_id");
        let mut items = Vec::new();
        loop {
            let rows = self
                .command(
                    "player_queues/items",
                    json!({"queue_id":id,"limit":500,"offset":items.len()}),
                )
                .await?;
            let rows = rows
                .as_array()
                .ok_or_else(|| anyhow!("Invalid queue items"))?;
            items.extend(rows.iter().map(queue_item));
            if rows.len() < 500
                || v["items"]
                    .as_u64()
                    .is_some_and(|total| items.len() as u64 >= total)
            {
                break;
            }
        }
        let current = queue_item(&v["current_item"]);
        Ok(Queue {
            details: v.clone(),
            id,
            name: text(&v, "display_name"),
            state: text(&v, "state"),
            current_title: current.title,
            current_artist: current.artist,
            elapsed: v["elapsed_time"].as_f64().unwrap_or_default(),
            duration: current.duration,
            items,
        })
    }
    pub async fn control(&self, player_id: &str, action: Control) -> Result<()> {
        match action {
            Control::Volume(v) if v > 100 => {
                return Err(anyhow!("Volume must be between 0 and 100"))
            }
            Control::Seek(v) if !v.is_finite() || v < 0.0 || v >= u64::MAX as f64 => {
                return Err(anyhow!("Seek position must be finite and nonnegative"))
            }
            _ => {}
        }
        if let Control::Volume(volume) = action {
            self.command(
                "players/cmd/volume_set",
                json!({"player_id":player_id,"volume_level":volume}),
            )
            .await?;
            return Ok(());
        }
        let q = self.active_queue(player_id).await?;
        let id = text(&q, "queue_id");
        let (command, args) = match action {
            Control::Toggle => ("player_queues/play_pause", json!({"queue_id":id})),
            Control::Next => ("player_queues/next", json!({"queue_id":id})),
            Control::Previous => ("player_queues/previous", json!({"queue_id":id})),
            Control::Seek(position) => (
                "player_queues/seek",
                json!({"queue_id":id,"position":position as u64}),
            ),
            Control::Volume(_) => unreachable!(),
        };
        self.command(command, args).await?;
        Ok(())
    }
    /// Replace the active queue and start playback immediately.
    pub async fn play_uri(&self, player_id: &str, uri: &str) -> Result<()> {
        self.media(player_id, uri, "replace").await
    }
    /// Append to the active queue without replacing its contents.
    pub async fn enqueue_uri(&self, player_id: &str, uri: &str) -> Result<()> {
        self.media(player_id, uri, "add").await
    }
    async fn media(&self, player_id: &str, uri: &str, option: &str) -> Result<()> {
        let q = self.active_queue(player_id).await?;
        self.command(
            "player_queues/play_media",
            json!({"queue_id":text(&q,"queue_id"),"media":uri,"option":option}),
        )
        .await?;
        Ok(())
    }
    pub async fn search(&self, query: &str) -> Result<Vec<Track>> {
        let v = self
            .command(
                "music/search",
                json!({"search_query":query,"media_types":["track","album","artist","playlist","radio","audiobook","podcast"],"limit":50}),
            )
            .await?;
        if !v["tracks"].is_array() {
            return Err(anyhow!("Invalid search results"));
        }
        let mut results = Vec::new();
        for (field, label) in [
            ("tracks", ""),
            ("albums", "Album"),
            ("artists", "Artist"),
            ("playlists", "Playlist"),
            ("radio", "Radio"),
            ("audiobooks", "Audiobook"),
            ("podcasts", "Podcast"),
        ] {
            for item in v[field].as_array().into_iter().flatten().take(50) {
                results.push(Track {
                    uri: text(item, "uri"),
                    title: if label.is_empty() {
                        text(item, "name")
                    } else {
                        format!("[{label}] {}", text(item, "name"))
                    },
                    artist: artist(item),
                });
            }
        }
        Ok(results)
    }

    pub async fn players(&self) -> Result<Vec<Player>> {
        let value = self.command("players/all", json!({})).await?;
        let rows = value
            .as_array()
            .ok_or_else(|| anyhow!("Invalid player list"))?;
        Ok(rows
            .iter()
            .map(|v| Player {
                details: v.clone(),
                id: text(v, "player_id"),
                name: text(v, "name"),
                state: text(v, "playback_state"),
                volume: v["volume_level"]
                    .as_u64()
                    .and_then(|n| u8::try_from(n).ok()),
                available: v["available"].as_bool().unwrap_or(false),
            })
            .collect())
    }
}
fn artist(v: &Value) -> String {
    v["artists"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|v| text(v, "name"))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}
fn queue_item(v: &Value) -> QueueItem {
    let title = text(&v["media_item"], "name");
    QueueItem {
        id: text(v, "queue_item_id"),
        title: if title.is_empty() {
            text(v, "name")
        } else {
            title
        },
        artist: artist(&v["media_item"]),
        duration: v["duration"].as_f64().unwrap_or_default(),
    }
}
fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().into()
}
