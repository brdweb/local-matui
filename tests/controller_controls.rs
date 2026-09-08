use local_matui::{
    api::ApiClient,
    controller::{Controller, Request, Update},
    ui::Action,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test]
async fn player_commands_reach_the_server_once_and_expired_ones_are_dropped() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        "fixture",
    )
    .unwrap();
    let (observed, mut observations) = tokio::sync::mpsc::channel(8);
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = Vec::new();
            loop {
                let mut bytes = [0; 4096];
                let n = socket.read(&mut bytes).await.unwrap();
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&bytes[..n]);
                if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&data[i + 4..]) {
                        let body = match v["command"].as_str().unwrap() {
                            "players/all" => {
                                r#"[{"player_id":"p","volume_level":98,"available":true}]"#
                            }
                            "players/cmd/volume_up" => {
                                observed.send(v.clone()).await.unwrap();
                                "null"
                            }
                            other => panic!("unexpected {other}"),
                        };
                        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
                        break;
                    }
                }
            }
        }
    });
    // The server owns the volume step, so no player state is read first.
    let step = || {
        Action::Command(local_matui::controls::Command::Player {
            name: "volume_up",
            args: serde_json::json!({}),
        })
    };
    let mut worker = Controller::start(api);
    worker
        .requests
        .send(Request::new(Some("p".into()), step()))
        .await
        .unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), observations.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result["args"]["player_id"], "p");
    let mut expired = Request::new(Some("p".into()), step());
    expired.issued -= std::time::Duration::from_secs(5);
    worker.requests.send(expired).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Update::Notice(text) = worker.updates.recv().await.unwrap() {
                if text.contains("expired") {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();
    assert!(observations.try_recv().is_err());
    worker.shutdown().await;
    server.abort();
}
