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
    Browse(
        u64,
        Result<(Vec<crate::music::Media>, Option<crate::music::Target>), String>,
    ),
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

/// One in-flight read, cancelled when a newer one supersedes it or the
/// controller stops, so an abandoned request does not hold a connection open.
#[derive(Default)]
struct Pending(Option<JoinHandle<()>>);

impl Pending {
    fn replace(&mut self, task: JoinHandle<()>) {
        if let Some(previous) = self.0.replace(task) {
            previous.abort();
        }
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }
}

impl Controller {
    pub fn start(api: ApiClient) -> Self {
        let (selection, mut selected) = watch::channel::<Option<String>>(None);
        let (requests, mut commands) = mpsc::channel::<Request>(32);
        let (tx, updates) = mpsc::channel(16);
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // Reading the library must never delay a transport key, so browse
            // and search run beside this loop. Only the newest of each matters:
            // the interface discards stale replies by generation, so a superseded
            // request is cancelled rather than left to finish unread.
            let (mut browsing, mut searching) = (Pending::default(), Pending::default());
            loop {
                tokio::select! {
                    _ = interval.tick() => {},
                    changed = selected.changed() => { if changed.is_err() { break; } },
                    command = commands.recv() => {
                        let Some(command) = command else { break; };
                        if command.issued.elapsed() > Duration::from_secs(3) && !matches!(command.action, Action::Browse {..} | Action::Search(_)) {
                            let _ = tx.send(Update::Notice("Command expired; press the key again".into())).await;
                            continue;
                        }
                        if let Action::Search(query) = command.action {
                            let (api, tx) = (api.clone(), tx.clone());
                            searching.replace(tokio::spawn(async move {
                                let result = api.search(&query).await.map_err(|e|e.to_string());
                                let _ = tx.send(Update::Search(query,result)).await;
                            }));
                            continue;
                        }
                        if let Action::Browse {generation, target} = command.action {
                            let (api, tx) = (api.clone(), tx.clone());
                            browsing.replace(tokio::spawn(async move {
                                let result = api.browse(&target).await.map_err(|e|e.to_string());
                                let _ = tx.send(Update::Browse(generation,result)).await;
                            }));
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
                // The player list and the selected queue are independent reads,
                // so they cost one round trip together rather than two in turn.
                let id = selected.borrow().clone();
                let (players, queue) = match &id {
                    Some(id) => {
                        let (players, queue) = tokio::join!(api.players(), api.queue(id));
                        (players, Some(queue.map_err(|e| e.to_string())))
                    }
                    None => (api.players().await, None),
                };
                let players = match players {
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
                if let (Some(id), Some(queue)) = (id, queue) {
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
        // The interface resolves the target position, so holding the key does
        // not issue a queue request per keystroke.
        Action::Seek(position) => api.control(&player, Control::Seek(position)).await,
        Action::Play(uri) => api.play_uri(&player, &uri).await,
        Action::Enqueue(uri) => api.enqueue_uri(&player, &uri).await,
        Action::PlayNext(uri) => api.play_next_uri(&player, &uri).await,
        Action::Command(command) => api.playback_command(&player, command).await,
        _ => Ok(()),
    }
}
