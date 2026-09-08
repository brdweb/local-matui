use matui::{
    api::ApiClient,
    controller::{Controller, Request, Update},
    ui::Action,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test]
async fn worker_returns_search_results_without_selecting_or_playing_a_player() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        "fixture",
    )
    .unwrap();
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
                            "players/all" => "[]",
                            "music/search" => {
                                r#"{"tracks":[{"uri":"library://track/1","name":"Found"}]}"#
                            }
                            other => panic!("unexpected mutation: {other}"),
                        };
                        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
                        break;
                    }
                }
            }
        }
    });
    let mut worker = Controller::start(api);
    worker
        .requests
        .send(Request::new(None, Action::Search("Found".into())))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Update::Search(query, Ok(items)) = worker.updates.recv().await.unwrap() {
                assert_eq!(query, "Found");
                assert_eq!(items[0].title, "Found");
                break;
            }
        }
    })
    .await
    .unwrap();
    worker.shutdown().await;
    server.abort();
}
