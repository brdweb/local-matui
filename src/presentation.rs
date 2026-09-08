use crate::{
    controller::Update,
    ui::{App, PlayerView, TrackView},
};

fn display(text: String) -> String {
    text.chars().filter(|c| !c.is_control()).take(512).collect()
}

/// Apply network snapshots only to the player/query they were requested for.
pub fn apply(app: &mut App, event: Update) {
    match event {
        Update::Players(players) => {
            let cursor_id = app.players.get(app.player_cursor).map(|p| p.id.clone());
            app.players = players
                .into_iter()
                .map(|p| PlayerView {
                    details: p.details,
                    id: p.id,
                    name: display(p.name),
                    state: display(p.state),
                    volume: p.volume,
                    available: p.available,
                })
                .collect();
            app.player_cursor = cursor_id
                .and_then(|id| app.players.iter().position(|p| p.id == id))
                .unwrap_or(0);
            if !app.connected {
                app.status = "Connected · select a player; controls act on that player".into();
            }
            app.connected = true;
        }
        Update::Queue(id, result) if app.selected_id.as_deref() == Some(id.as_str()) => {
            match result {
                Ok(queue) => {
                    let highlighted = app.queue.get(app.queue_cursor).map(|t| t.id.clone());
                    app.queue_id = queue.id;
                    app.queue_details = queue.details;
                    app.title = if queue.current_title.is_empty() {
                        "Nothing playing".into()
                    } else {
                        display(queue.current_title)
                    };
                    app.artist = display(queue.current_artist);
                    app.elapsed = queue.elapsed;
                    app.duration = queue.duration;
                    app.queue = queue
                        .items
                        .into_iter()
                        .map(|t| TrackView {
                            id: t.id,
                            title: display(t.title),
                            artist: display(t.artist),
                            duration: t.duration,
                            ..Default::default()
                        })
                        .collect();
                    app.queue_cursor = highlighted
                        .and_then(|id| app.queue.iter().position(|t| t.id == id))
                        .unwrap_or(app.queue_cursor.min(app.queue.len().saturating_sub(1)));
                }
                Err(error) => {
                    app.queue_id.clear();
                    app.queue_details = serde_json::Value::Null;
                    app.queue.clear();
                    app.title = "Queue unavailable".into();
                    app.artist.clear();
                    app.elapsed = 0.0;
                    app.duration = 0.0;
                    app.status = error;
                }
            }
        }
        Update::Search(query, result) if query == app.query.trim() => match result {
            Ok(tracks) => {
                app.results = tracks
                    .into_iter()
                    .map(|t| TrackView {
                        uri: t.uri,
                        title: display(t.title),
                        artist: display(t.artist),
                        ..Default::default()
                    })
                    .collect();
                app.search_cursor = 0;
                app.status = format!(
                    "Search complete · {} results (up to 50 per type)",
                    app.results.len()
                );
            }
            Err(error) => {
                app.results.clear();
                app.status = error;
            }
        },
        Update::Offline(error) => {
            app.connected = false;
            app.status = format!("Disconnected · data stale · retrying: {error}");
        }
        Update::Notice(text) => app.status = text,
        _ => {}
    }
}
