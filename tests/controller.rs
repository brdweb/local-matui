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
