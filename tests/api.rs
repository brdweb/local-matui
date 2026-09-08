#[path = "../src/api.rs"]
mod api;
use api::ApiClient;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn server(replies: Vec<(u16, String)>) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/prefix", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (status, body) in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let end;
            loop {
                let mut b = [0; 1024];
                let n = socket.read(&mut b).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    end = i + 4;
                    break;
                }
            }
            let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
            assert!(headers.starts_with("post /prefix/api http/1.1"));
            assert!(headers.contains("authorization: bearer test-secret\r\n"));
            let len: usize = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .unwrap()
                .parse()
                .unwrap();
            while bytes.len() < end + len {
                let mut b = [0; 1024];
                let n = socket.read(&mut b).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
            }
            requests.push(serde_json::from_slice(&bytes[end..end + len]).unwrap());
            socket.write_all(format!("HTTP/1.1 {status} Test\r\nLocation: /prefix/api\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
        requests
    });
    (url, task)
}
#[test]
fn rejects_unsafe_urls_and_tokens_without_echoing_them() {
    for url in [
        "ftp://localhost",
        "http://user:secret@localhost",
        "http://@localhost",
        "http://localhost/?secret",
        "http://localhost/#secret",
        "not a url",
    ] {
        assert!(
            ApiClient::new(url, "test-secret").is_err(),
            "accepted {url}"
        );
    }
    assert!(ApiClient::new("http://localhost", "bad\r\ntoken").is_err());
    assert!(ApiClient::new("http://localhost", "").is_err());
}

#[tokio::test]
async fn queue_resolves_group_owner_and_loads_items() {
    let item = json!({"queue_item_id":"i1","name":"Fallback","duration":120,"media_item":{"name":"Song","artists":[{"name":"Artist"},{"name":"Guest"}]}});
    let (url, task) = server(vec![ok(json!({"queue_id":"leader","display_name":"Group","state":"playing","elapsed_time":12.5,"items":1,"current_item":item})), ok(json!([item]))]).await;
    let q = ApiClient::new(&url, "test-secret")
        .unwrap()
        .queue("member")
        .await
        .unwrap();
    assert_eq!(
        (
            &*q.id,
            &*q.name,
            &*q.state,
            &*q.current_title,
            &*q.current_artist,
            q.elapsed,
            q.duration
        ),
        (
            "leader",
            "Group",
            "playing",
            "Song",
            "Artist, Guest",
            12.5,
            120.0
        )
    );
    assert_eq!(
        (
            &*q.items[0].id,
            &*q.items[0].title,
            &*q.items[0].artist,
            q.items[0].duration
        ),
        ("i1", "Song", "Artist, Guest", 120.0)
    );
    let r = task.await.unwrap();
    assert_eq!(r[0]["command"], "player_queues/get_active_queue");
    assert_eq!(r[0]["args"], json!({"player_id":"member"}));
    assert_eq!(r[1]["command"], "player_queues/items");
    assert_eq!(
        r[1]["args"],
        json!({"queue_id":"leader","limit":500,"offset":0})
    );
}

#[tokio::test]
async fn search_tracks_with_artist_names() {
    let (url, task) = server(vec![ok(
        json!({"tracks":[{"uri":"library://track/1","name":"Song","artists":[{"name":"Artist"}]}]}),
    )])
    .await;
    let t = ApiClient::new(&url, "test-secret")
        .unwrap()
        .search("Song")
        .await
        .unwrap();
    assert_eq!(
        (&*t[0].uri, &*t[0].title, &*t[0].artist),
        ("library://track/1", "Song", "Artist")
    );
    let r = task.await.unwrap();
    assert_eq!(r[0]["command"], "music/search");
    assert_eq!(
        r[0]["args"],
        json!({"search_query":"Song","media_types":["track"],"limit":50})
    );
}
#[tokio::test]
async fn controls_route_transport_to_active_queue_but_volume_to_player() {
    use api::Control;
    for (action, command, args) in [
        (
            Control::Toggle,
            "player_queues/play_pause",
            json!({"queue_id":"leader"}),
        ),
        (
            Control::Next,
            "player_queues/next",
            json!({"queue_id":"leader"}),
        ),
        (
            Control::Previous,
            "player_queues/previous",
            json!({"queue_id":"leader"}),
        ),
        (
            Control::Seek(42.8),
            "player_queues/seek",
            json!({"queue_id":"leader","position":42}),
        ),
        (
            Control::Volume(23),
            "players/cmd/volume_set",
            json!({"player_id":"member","volume_level":23}),
        ),
    ] {
        let replies = if matches!(action, Control::Volume(_)) {
            vec![ok(Value::Null)]
        } else {
            vec![ok(json!({"queue_id":"leader"})), ok(Value::Null)]
        };
        let (url, task) = server(replies).await;
        ApiClient::new(&url, "test-secret")
            .unwrap()
            .control("member", action)
            .await
            .unwrap();
        let r = task.await.unwrap();
        assert_eq!(r.last().unwrap()["command"], command);
        assert_eq!(r.last().unwrap()["args"], args);
    }
}
#[tokio::test]
async fn play_explicitly_replaces_while_enqueue_adds() {
    for option in ["replace", "add"] {
        let (url, task) = server(vec![ok(json!({"queue_id":"leader"})), ok(Value::Null)]).await;
        let client = ApiClient::new(&url, "test-secret").unwrap();
        if option == "replace" {
            client
                .play_uri("member", "library://track/1")
                .await
                .unwrap();
        } else {
            client
                .enqueue_uri("member", "library://track/1")
                .await
                .unwrap();
        }
        let r = task.await.unwrap();
        assert_eq!(r[1]["command"], "player_queues/play_media");
        assert_eq!(
            r[1]["args"],
            json!({"queue_id":"leader","media":"library://track/1","option":option})
        );
    }
}
#[tokio::test]
async fn redirects_are_not_followed_and_client_can_retry() {
    let (url, task) = server(vec![(307, "test-secret".into()), ok(json!([]))]).await;
    let client = ApiClient::new(&url, "test-secret").unwrap();
    assert!(client.players().await.is_err());
    assert!(client.players().await.unwrap().is_empty());
    assert_eq!(task.await.unwrap().len(), 2);
}
#[tokio::test]
async fn stalled_requests_time_out_without_exposing_url_or_token() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/private-server-path",
        listener.local_addr().unwrap()
    );
    let task = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    });
    let client = ApiClient::new(&url, "test-secret").unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(12), client.players()).await;
    task.abort();
    let error = format!(
        "{:#}",
        result
            .expect("client must enforce its own timeout")
            .unwrap_err()
    );
    assert!(!error.contains("test-secret") && !error.contains("private-server-path"));
}
#[tokio::test]
async fn queue_paginates_beyond_first_500_items() {
    let page: Vec<Value> = (0..500)
        .map(|i| json!({"queue_item_id":i.to_string(),"name":"Song"}))
        .collect();
    let (url, task) = server(vec![
        ok(json!({"queue_id":"leader","items":501})),
        ok(json!(page)),
        ok(json!([{"queue_item_id":"500","name":"Last"}])),
    ])
    .await;
    let q = ApiClient::new(&url, "test-secret")
        .unwrap()
        .queue("member")
        .await
        .unwrap();
    assert_eq!(q.items.len(), 501);
    assert_eq!(q.items[500].title, "Last");
    assert_eq!(task.await.unwrap()[2]["args"]["offset"], 500);
}
#[tokio::test]
async fn invalid_control_values_are_rejected_before_network() {
    for action in [
        api::Control::Volume(101),
        api::Control::Seek(-1.0),
        api::Control::Seek(f64::NAN),
        api::Control::Seek(f64::INFINITY),
    ] {
        let (url, task) = server(vec![ok(json!({"queue_id":"leader"})), ok(Value::Null)]).await;
        let error = ApiClient::new(&url, "test-secret")
            .unwrap()
            .control("member", action)
            .await;
        task.abort();
        assert!(error.is_err());
    }
}
#[test]
fn client_debug_is_redacted() {
    let client = ApiClient::new("http://localhost/private-path", "test-secret").unwrap();
    let debug = format!("{client:?}");
    assert!(!debug.contains("test-secret") && !debug.contains("private-path"));
}
// Supplemental regression cases for the already exercised transport/parser.
#[tokio::test]
async fn http_errors_malformed_json_and_wrong_envelopes_are_redacted() {
    for response in [
        (401, "test-secret private-body".into()),
        (500, "test-secret private-body".into()),
        (200, "test-secret private-body".into()),
        ok(json!({"result":[],"error":"test-secret private-body"})),
    ] {
        let (url, task) = server(vec![response]).await;
        let error = ApiClient::new(&url, "test-secret")
            .unwrap()
            .players()
            .await
            .unwrap_err();
        for message in [format!("{error:#}"), format!("{error:?}")] {
            assert!(!message.contains("test-secret") && !message.contains("private-body"));
        }
        task.await.unwrap();
    }
}
#[tokio::test]
async fn no_active_queue_is_an_error_not_a_guessed_player_queue() {
    let (url, task) = server(vec![ok(Value::Null)]).await;
    assert!(ApiClient::new(&url, "test-secret")
        .unwrap()
        .play_uri("member", "library://track/1")
        .await
        .is_err());
    assert_eq!(task.await.unwrap().len(), 1);
}
#[tokio::test]
async fn optional_metadata_can_be_missing_or_null() {
    let (url, task) = server(vec![
        ok(json!([{"player_id":"p","name":"Offline","available":false,"volume_level":null}])),
        ok(json!({"queue_id":"q","current_item":null,"items":1})),
        ok(json!([{"queue_item_id":"radio","name":"Station","duration":null,"media_item":null}])),
    ])
    .await;
    let client = ApiClient::new(&url, "test-secret").unwrap();
    let p = client.players().await.unwrap();
    assert_eq!(p[0].volume, None);
    assert!(!p[0].available);
    let q = client.queue("p").await.unwrap();
    assert!(q.current_title.is_empty());
    assert_eq!(q.items[0].title, "Station");
    assert_eq!(q.items[0].duration, 0.0);
    task.await.unwrap();
}
fn ok(v: Value) -> (u16, String) {
    (200, v.to_string())
}

#[tokio::test]
async fn players_reads_bare_http_result_not_websocket_envelope() {
    let (url, task) = server(vec![ok(json!([{"player_id":"p1","name":"Kitchen","playback_state":"playing","volume_level":37,"available":true}]))]).await;
    let players = ApiClient::new(&url, "test-secret")
        .unwrap()
        .clone()
        .players()
        .await
        .unwrap();
    assert_eq!(players.len(), 1);
    assert_eq!(
        (
            &*players[0].id,
            &*players[0].name,
            &*players[0].state,
            players[0].volume,
            players[0].available
        ),
        ("p1", "Kitchen", "playing", Some(37), true)
    );
    let requests = task.await.unwrap();
    assert_eq!(requests[0]["command"], "players/all");
    assert!(requests[0]["message_id"].is_string());
}
