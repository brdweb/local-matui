use crate::api::{ApiClient, Control, Player, Queue, Track};
use crate::ui::Action;
use std::time::Duration;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

pub enum Update {
    Players(Vec<Player>),
    Queue(String, Result<Queue, String>),
    Offline(String),
    Search(String, Result<Vec<Track>, String>),
    Notice(String),
}

pub struct Request {
    pub player: Option<String>,
    pub action: Action,
    pub issued: std::time::Instant,
}
impl Request {
    pub fn new(player: Option<String>, action: Action) -> Self {
        Self {
            player,
            action,
            issued: std::time::Instant::now(),
        }
    }
}

pub struct Controller {
    pub requests: mpsc::Sender<Request>,
    pub selection: watch::Sender<Option<String>>,
    pub updates: mpsc::Receiver<Update>,
    task: JoinHandle<()>,
}

impl Controller {
    pub fn start(api: ApiClient) -> Self {
        let (selection, mut selected) = watch::channel::<Option<String>>(None);
        let (requests, mut commands) = mpsc::channel::<Request>(8);
        let (tx, updates) = mpsc::channel(16);
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = interval.tick() => {},
                    changed = selected.changed() => { if changed.is_err() { break; } },
                    command = commands.recv() => {
                        let Some(command) = command else { break; };
                        if command.issued.elapsed() > Duration::from_secs(3) {
                            let _ = tx.send(Update::Notice("Command expired; press the key again".into())).await;
                            continue;
                        }
                        if let Action::Search(query) = command.action {
                            let result = api.search(&query).await.map_err(|e|e.to_string());
                            if tx.send(Update::Search(query,result)).await.is_err() { break; }
                            continue;
                        }
                        if !matches!(command.action, Action::Refresh) {
                            let result = execute(&api,command).await;
                            let notice = match result {
                                Ok(()) => "Command accepted; refreshing state".into(),
                                Err(e) => format!("Command failed (not retried): {e}"),
                            };
                            if tx.send(Update::Notice(notice)).await.is_err() { break; }
                        }
                    }
                }
                let players = match api.players().await {
                    Ok(players) => players,
                    Err(err) => {
                        if tx.send(Update::Offline(err.to_string())).await.is_err() {
                            break;
                        }
                        continue;
                    }
                };
                if tx.send(Update::Players(players)).await.is_err() {
                    break;
                }
                let id = selected.borrow().clone();
                if let Some(id) = id {
                    let queue = api.queue(&id).await.map_err(|e| e.to_string());
                    if tx.send(Update::Queue(id, queue)).await.is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            requests,
            selection,
            updates,
            task,
        }
    }
    pub async fn shutdown(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}

async fn execute(api: &ApiClient, request: Request) -> anyhow::Result<()> {
    let player = request
        .player
        .ok_or_else(|| anyhow::anyhow!("No player selected"))?;
    match request.action {
        Action::Toggle => api.control(&player, Control::Toggle).await,
        Action::Next => api.control(&player, Control::Next).await,
        Action::Previous => api.control(&player, Control::Previous).await,
        Action::Volume(delta) => {
            let current = api
                .players()
                .await?
                .into_iter()
                .find(|p| p.id == player && p.available)
                .and_then(|p| p.volume)
                .ok_or_else(|| anyhow::anyhow!("Player volume unavailable"))?;
            api.control(
                &player,
                Control::Volume((current as i16 + delta as i16).clamp(0, 100) as u8),
            )
            .await
        }
        Action::Seek(delta) => {
            let queue = api.queue(&player).await?;
            if queue.duration <= 0.0 {
                anyhow::bail!("This item has no seekable duration");
            }
            api.control(
                &player,
                Control::Seek((queue.elapsed + delta as f64).clamp(0.0, queue.duration)),
            )
            .await
        }
        Action::Play(uri) => api.play_uri(&player, &uri).await,
        Action::Enqueue(uri) => api.enqueue_uri(&player, &uri).await,
        Action::Command(command) => api.playback_command(&player, command).await,
        _ => Ok(()),
    }
}
