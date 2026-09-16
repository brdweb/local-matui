use local_matui::{
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
    let mut controller = Controller::start(client);
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
    use local_matui::{controller::Request, music::Target, ui::Action};
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
    let mut controller = Controller::start(client);
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
