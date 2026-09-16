use ma_tui::{
    api::ApiClient,
    controller::{Controller, Update},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test]
async fn worker_polls_without_blocking_caller_and_can_cancel_stalled_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        "fixture-token",
    )
    .unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 4096];
        assert!(socket.read(&mut buffer).await.unwrap() > 0);
        let body = r#"[{"player_id":"test","name":"Fixture player","available":true}]"#;
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let (_socket, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    let mut controller = Controller::start(client, None, false);
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), controller.updates.recv())
        .await
        .unwrap()
        .unwrap();
    match event {
        Update::Players(players) => assert_eq!(players[0].id, "test"),
        _ => panic!("expected players"),
    }
    controller.selection.send(Some("test".into())).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), controller.shutdown())
        .await
        .unwrap();
    server.abort();
}

/// A library read that never answers must not hold up transport or polling.
#[tokio::test]
async fn a_stalled_browse_does_not_block_the_poll() {
    use ma_tui::{controller::Request, music::Target, ui::Action};
    use std::time::Duration;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        "fixture-token",
    )
    .unwrap();
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buffer = vec![0; 8192];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                if String::from_utf8_lossy(&buffer[..read]).contains("music/browse") {
                    // The slow read: answer it never.
                    std::future::pending::<()>().await;
                }
                let body = r#"[{"player_id":"test","name":"Fixture player","available":true}]"#;
                let _ = socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .await;
            });
        }
    });
    let mut controller = Controller::start(client, None, false);
    let first = tokio::time::timeout(Duration::from_secs(2), controller.updates.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(first, Update::Players(_)), "the poll starts");

    controller
        .requests
        .send(Request::new(
            None,
            Action::Browse {
                generation: 1,
                target: Target::Providers { path: None },
            },
        ))
        .await
        .unwrap();

    let players = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match controller.updates.recv().await {
                Some(Update::Players(players)) => return players,
                Some(_) => continue,
                None => panic!("the controller stopped"),
            }
        }
    })
    .await
    .expect("the poll must keep running while a library read is outstanding");
    assert_eq!(players[0].id, "test");

    tokio::time::timeout(Duration::from_secs(1), controller.shutdown())
        .await
        .unwrap();
    server.abort();
}

/// Events drive the reads: a position for the queue on screen costs no request
/// at all, and a queue that is not on screen costs nothing either.
#[tokio::test]
async fn events_route_by_queue_and_a_position_needs_no_request() {
    use ma_tui::events::Event;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        "fixture-token",
    )
    .unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let counted = requests.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let counted = counted.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0; 8192];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                counted.fetch_add(1, Ordering::Release);
                let body = if request.contains("get_active_queue") {
                    r#"{"queue_id":"q1","display_name":"Kitchen","state":"playing","items":0,"elapsed_time":5.0,"current_item":{}}"#
                } else if request.contains("player_queues/items") {
                    "[]"
                } else {
                    r#"[{"player_id":"p1","name":"Kitchen","available":true}]"#
                };
                let _ = socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .await;
            });
        }
    });

    let (feed, stream) = tokio::sync::mpsc::channel(16);
    let mut controller = Controller::start(client, Some(stream), false);
    controller.selection.send(Some("p1".into())).unwrap();

    // Wait until a queue has actually been read, so the controller knows which
    // queue is on screen.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(Update::Queue(_, Ok(queue))) = controller.updates.recv().await {
                assert_eq!(queue.id, "q1");
                return;
            }
        }
    })
    .await
    .expect("the selected queue is read once");
    // Startup reads twice — the first tick and the selection change — so let
    // those finish before counting. The live interval is far away.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let settled = requests.load(Ordering::Acquire);

    // A position for a different queue is not ours to show.
    feed.send(Event::Elapsed("other".into(), 99.0))
        .await
        .unwrap();
    // The one for our queue is, and it arrives without asking the server.
    feed.send(Event::Elapsed("q1".into(), 42.0)).await.unwrap();
    let elapsed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match controller.updates.recv().await {
                Some(Update::Elapsed(id, seconds)) => return (id, seconds),
                Some(_) => continue,
                None => panic!("the controller stopped"),
            }
        }
    })
    .await
    .expect("a position event must reach the interface");
    assert_eq!(
        elapsed,
        ("q1".into(), 42.0),
        "only the displayed queue's position is shown"
    );
    assert_eq!(
        requests.load(Ordering::Acquire),
        settled,
        "a position costs no request"
    );

    tokio::time::timeout(Duration::from_secs(2), controller.shutdown())
        .await
        .unwrap();
    server.abort();
}
