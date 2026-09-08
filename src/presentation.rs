use crate::{
    controller::Update,
    ui::{App, PlayerView, TrackView},
};

fn display(text: String) -> String {
    text.chars().filter(|c| !c.is_control()).take(512).collect()
}

/// MA can expose the embedded Sendspin endpoint behind a universal player.
/// Select its public player ID, keeping control/queue routing on that wrapper.
pub fn select_local(app: &mut App, endpoint: &str) -> Option<String> {
    if app.selected_id.is_some() || !app.connected || endpoint.is_empty() {
        return None;
    }
    let index = app
        .players
        .iter()
        .position(|p| p.available && matches_endpoint(p, endpoint))?;
    let id = app.players[index].id.clone();
    app.selected_id = Some(id.clone());
    app.player_cursor = index;
    Some(id)
}

/// Whether a player is Matui's own endpoint, directly or as the universal
/// wrapper MA puts in front of it. Display names are never identity matches.
pub fn matches_endpoint(player: &PlayerView, endpoint: &str) -> bool {
    !endpoint.is_empty()
        && (player.id == endpoint
            || player.details["output_protocols"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|v| v["output_protocol_id"].as_str() == Some(endpoint)))
}

/// Apply network snapshots only to the player/query they were requested for.
pub fn apply(app: &mut App, event: Update) {
    match event {
        Update::Browse(generation, result) => app.music.apply(generation, result),
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
                        media: t.media,
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
