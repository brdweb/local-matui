//! Opt-in integration with the real CPAL/ALSA null device. No audible output.
use futures_util::{SinkExt, StreamExt};
use matui::audio::{self, AudioConfig};
use matui::visualizer::Analyzer;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Linux ALSA null output; run explicitly"]
async fn opens_real_cpal_null_stream_and_acknowledges_volume() {
    check_output(Some("alsa:null")).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a desktop default output; opens a silent stream with no audio frames"]
async fn opens_default_output_silently_and_acknowledges_volume() {
    check_output(None).await;
}

async fn check_output(device: Option<&str>) {
    assert!(audio::devices()
        .unwrap()
        .iter()
        .any(|d| d.id == "alsa:null"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (configured, wait_configured) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let auth = ws.next().await.unwrap().unwrap();
        let auth: serde_json::Value = serde_json::from_str(auth.to_text().unwrap()).unwrap();
        assert_eq!(auth["type"], "auth");
        ws.send(Message::text(r#"{"type":"auth_ok"}"#))
            .await
            .unwrap();
        let hello = ws.next().await.unwrap().unwrap();
        let hello: serde_json::Value = serde_json::from_str(hello.to_text().unwrap()).unwrap();
        assert_eq!(hello["type"], "client/hello");
        ws.send(Message::text(r#"{"type":"server/hello","payload":{"server_id":"null-fixture","name":"Local null fixture","version":1,"active_roles":["player@v1"],"connection_reason":"playback"}}"#)).await.unwrap();
        ws.send(Message::text(r#"{"type":"stream/start","payload":{"player":{"codec":"pcm","channels":2,"sample_rate":48000,"bit_depth":16}}}"#)).await.unwrap();
        wait_configured.await.unwrap();
        ws.send(Message::text(
            r#"{"type":"server/command","payload":{"player":{"command":"volume","volume":21}}}"#,
        ))
        .await
        .unwrap();
        while let Some(Ok(message)) = ws.next().await {
            if let Message::Text(text) = message {
                let v: serde_json::Value = serde_json::from_str(&text).unwrap();
                if v["type"] == "client/state" && v["payload"]["player"]["volume"] == 21 {
                    return;
                }
            }
        }
        panic!("no volume acknowledgement");
    });
    let spectrum = Analyzer::new();
    let mut audio = audio::start(
        AudioConfig {
            server: base,
            token: "fixture".into(),
            player_id: "null-fixture".into(),
            player_name: "Null fixture".into(),
            device_id: device.map(str::to_owned),
            volume: 30,
            muted: false,
        },
        Some(Arc::new(spectrum.clone())),
    )
    .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status = audio.status.borrow().clone();
            assert_ne!(status.state, "failed", "{}", status.detail);
            if status.detail == "Audio stream configured" {
                break;
            }
            audio.status.changed().await.unwrap();
        }
        configured.send(()).unwrap();
        server.await.unwrap();
    })
    .await;
    audio.shutdown().await;
    result.unwrap();
    // This fixture configures a stream but sends no audio frames, so the
    // visualizer must have nothing to show rather than something invented.
    assert_eq!(spectrum.buffered(), 0);
    assert!(spectrum.capture(Instant::now()).is_err());
}
