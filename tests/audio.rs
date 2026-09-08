// Offline fixtures only: no Music Assistant server or physical audio is used.
#[path = "../src/audio.rs"]
mod audio;

use futures_util::{SinkExt, StreamExt};
use std::time::Duration;

#[tokio::test]
async fn proxy_auth_is_first_frame_and_waits_for_auth_ok() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let fixture = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let auth: serde_json::Value =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(
            auth,
            serde_json::json!({"type":"auth", "token":"fixture-secret", "client_id":"fixture-id"})
        );
        assert!(tokio::time::timeout(Duration::from_millis(30), ws.next())
            .await
            .is_err());
        ws.send(tokio_tungstenite::tungstenite::Message::text(
            r#"{"type":"auth_ok"}"#,
        ))
        .await
        .unwrap();
    });
    audio::authenticate(
        &audio::proxy_url(&base).unwrap(),
        "fixture-secret",
        "fixture-id",
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    fixture.await.unwrap();
}

#[test]
fn pcm_decoder_validates_negotiated_format_and_frame_alignment() {
    use sendspin::protocol::messages::{AudioFormatSpec, StreamPlayerConfig};
    let supported = vec![AudioFormatSpec {
        codec: "pcm".into(),
        channels: 2,
        sample_rate: 48000,
        bit_depth: 16,
    }];
    let mut config = StreamPlayerConfig {
        codec: "pcm".into(),
        channels: 2,
        sample_rate: 48000,
        bit_depth: 16,
        codec_header: None,
    };
    let mut decoder = audio::StreamDecoder::new(&config, &supported).unwrap();
    assert_eq!(decoder.decode(&[0, 0, 0xff, 0x7f]).unwrap().len(), 2);
    assert!(decoder.decode(&[0, 0, 0]).is_err());
    config.channels = 0;
    assert!(audio::StreamDecoder::new(&config, &supported).is_err());
    config.channels = 2;
    config.codec_header = Some("fixture-secret-not-base64".into());
    let error = audio::StreamDecoder::new(&config, &supported)
        .err()
        .unwrap()
        .to_string();
    assert!(!error.contains("fixture-secret"));
    config.codec = "malicious-fixture-value".into();
    assert!(!audio::StreamDecoder::new(&config, &supported)
        .err()
        .unwrap()
        .to_string()
        .contains("malicious"));
}

// This output records decoded samples instead of opening CPAL. All transport,
// Sendspin negotiation, command processing, decoding and worker code is real.
struct FixtureOutput(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
impl audio::Output for FixtureOutput {
    fn formats(&self) -> anyhow::Result<Vec<sendspin::protocol::messages::AudioFormatSpec>> {
        Ok(vec![sendspin::protocol::messages::AudioFormatSpec {
            codec: "pcm".into(),
            channels: 2,
            sample_rate: 48000,
            bit_depth: 16,
        }])
    }
    fn begin(
        &mut self,
        _: sendspin::audio::AudioFormat,
        _: audio::SharedClock,
        _: audio::Gain,
    ) -> anyhow::Result<()> {
        self.0.lock().unwrap().push(format!(
            "begin:{}",
            std::thread::current().name().unwrap_or("unnamed")
        ));
        Ok(())
    }
    fn write(&mut self, buffer: sendspin::audio::AudioBuffer) {
        self.0
            .lock()
            .unwrap()
            .push(format!("samples:{}", buffer.samples.len()));
    }
    fn clear(&mut self) {
        self.0.lock().unwrap().push("clear".into());
    }
    fn gain(&mut self, gain: audio::Gain) {
        self.0.lock().unwrap().push(format!(
            "gain:{}:{}:{}",
            gain.volume, gain.muted, gain.delay
        ));
    }
    fn failed(&self) -> bool {
        false
    }
}
fn config(base: String) -> audio::AudioConfig {
    audio::AudioConfig {
        server: base,
        token: "fixture-secret".into(),
        player_id: "fixture-id".into(),
        player_name: "Fixture".into(),
        device_id: None,
        volume: 37,
        muted: true,
    }
}
async fn fixture_json<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>) -> serde_json::Value
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) = ws.next().await {
            return serde_json::from_str(&text).unwrap();
        }
    }
}
#[tokio::test]
async fn authenticated_session_decodes_on_worker_confirms_commands_and_stops() {
    use tokio_tungstenite::tungstenite::Message as Ws;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let output_events = events.clone();
    let assertions = events.clone();
    let (sent, received) = tokio::sync::oneshot::channel();
    let fixture = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        assert_eq!(fixture_json(&mut ws).await["type"], "auth");
        ws.send(Ws::text(r#"{"type":"auth_ok"}"#)).await.unwrap();
        let hello = fixture_json(&mut ws).await;
        assert_eq!(hello["type"], "client/hello");
        assert_eq!(hello["payload"]["client_id"], "fixture-id");
        ws.send(Ws::text(r#"{"type":"server/hello","payload":{"server_id":"offline-fixture","name":"Fixture","version":1,"active_roles":["player@v1"],"connection_reason":"playback"}}"#)).await.unwrap();
        loop {
            let state = fixture_json(&mut ws).await;
            if state["type"] == "client/state" {
                assert_eq!(state["payload"]["player"]["volume"], 37);
                break;
            }
        }
        ws.send(Ws::text(r#"{"type":"stream/start","payload":{"player":{"codec":"pcm","channels":2,"sample_rate":48000,"bit_depth":16}}}"#)).await.unwrap();
        // Wait for the real worker to acknowledge format setup before data.
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if output_events
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|x| x.starts_with("begin:"))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let mut chunk = vec![4u8];
        chunk.extend_from_slice(&0_i64.to_be_bytes());
        chunk.extend_from_slice(&[0, 0, 0xff, 0x7f]);
        ws.send(Ws::Binary(chunk.into())).await.unwrap();
        for (command, field, value) in [
            ("volume", "volume", serde_json::json!(62)),
            ("mute", "mute", serde_json::json!(false)),
            (
                "set_static_delay",
                "static_delay_ms",
                serde_json::json!(123),
            ),
        ] {
            ws.send(Ws::text(serde_json::json!({"type":"server/command","payload":{"player":{"command":command,field:value}}}).to_string())).await.unwrap();
            loop {
                let state = fixture_json(&mut ws).await;
                if state["type"] == "client/state" {
                    let key = if field == "mute" { "muted" } else { field };
                    assert_eq!(state["payload"]["player"][key], value);
                    break;
                }
            }
        }
        sent.send(()).unwrap();
        // Keep the socket alive until shutdown, so no reconnection races.
        while let Some(message) = ws.next().await {
            if matches!(message, Ok(Ws::Close(_)) | Err(_)) {
                break;
            }
        }
    });
    let handle =
        audio::start_with_output(config(base), move || Ok(FixtureOutput(events.clone()))).unwrap();
    tokio::time::timeout(Duration::from_secs(3), received)
        .await
        .unwrap()
        .unwrap();
    let mut status = handle.status.clone();
    tokio::time::timeout(Duration::from_secs(3), handle.shutdown())
        .await
        .unwrap();
    fixture.await.unwrap();
    let log = assertions.lock().unwrap();
    assert!(log.iter().any(|x| x == "begin:matui-audio"));
    assert!(log.iter().any(|x| x == "samples:2"));
    assert!(log.iter().any(|x| x == "gain:62:false:123"));
    assert_eq!(log.last().unwrap(), "clear");
    assert_eq!(status.borrow_and_update().state, "stopped");
}

#[test]
fn advertised_formats_are_pcm_first_and_device_compatible() {
    let formats =
        audio::formats_for_ranges(&[(2, 44100, 48000), (8, 48000, 96000), (1, 16000, 16000)]);
    assert!(!formats.is_empty());
    assert_eq!(formats[0].codec, "pcm");
    assert_eq!(formats[0].bit_depth, 16);
    assert!(formats.iter().all(
        |f| (f.channels == 2 && [44100, 48000].contains(&f.sample_rate))
            || (f.channels == 1 && f.sample_rate == 16000)
    ));
    assert!(formats.iter().any(|f| f.codec == "flac"));
    assert!(formats.iter().any(|f| f.codec == "opus"));
    assert!(formats
        .iter()
        .filter(|f| f.codec == "opus")
        .all(|f| f.sample_rate == 48000 && f.bit_depth == 16));
    assert!(audio::formats_for_ranges(&[(8, 48000, 48000)]).is_empty());
}
#[tokio::test]
async fn explicit_missing_device_fails_without_connecting_or_falling_back() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = config(format!("http://{}", listener.local_addr().unwrap()));
    config.device_id = Some("offline-fixture-missing-device".into());
    let mut handle = audio::start(config, None).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if handle.status.borrow().state == "failed" {
                break;
            }
            handle.status.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(!handle.status.borrow().detail.contains("fixture-secret"));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    handle.shutdown().await;
    // Listing devices is local enumeration; it must not initiate networking.
    let _local_devices: anyhow::Result<Vec<audio::AudioDevice>> = audio::devices();
    let sample = audio::AudioDevice {
        id: "fixture".into(),
        name: "Fixture".into(),
    };
    assert_eq!(sample.id, "fixture");
    assert_eq!(sample.name, "Fixture");
}

#[test]
fn compressed_offline_fixtures_decode_and_flac_header_must_match_negotiation() {
    use base64::Engine;
    use sendspin::protocol::messages::{AudioFormatSpec, StreamPlayerConfig};
    // Generated with ffmpeg anullsrc 48kHz stereo, s16 FLAC and libopus.
    // These are inert 10ms FLAC / 20ms Opus silence fixtures, not server captures.
    let header = "ZkxhQwAAACISABIAAAAAAEpYC7gC8AAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let mut spec = StreamPlayerConfig {
        codec: "flac".into(),
        channels: 2,
        sample_rate: 48000,
        bit_depth: 16,
        codec_header: Some(header.into()),
    };
    let supported = audio::formats_for_ranges(&[(2, 44100, 48000)]);
    let mut decoder = audio::StreamDecoder::new(&spec, &supported).unwrap();
    let bytes = base64::prelude::BASE64_STANDARD
        .decode("//h6GAAB36YAAAAAAAAZBw==")
        .unwrap();
    assert_eq!(decoder.decode(&bytes).unwrap().len(), 960);
    spec.sample_rate = 44100;
    assert!(
        audio::StreamDecoder::new(&spec, &supported).is_err(),
        "FLAC STREAMINFO disagrees with negotiated rate"
    );
    spec = StreamPlayerConfig {
        codec: "opus".into(),
        channels: 2,
        sample_rate: 48000,
        bit_depth: 16,
        codec_header: None,
    };
    let mut opus = audio::StreamDecoder::new(&spec, &supported).unwrap();
    assert_eq!(opus.decode(&[0xfc, 0xff, 0xfe]).unwrap().len(), 1920);
    assert!(opus.decode(&[]).is_err());
    let _type_check: Vec<AudioFormatSpec> = supported;
}

struct SlowFixture(FixtureOutput);
impl audio::Output for SlowFixture {
    fn formats(&self) -> anyhow::Result<Vec<sendspin::protocol::messages::AudioFormatSpec>> {
        self.0.formats()
    }
    fn begin(
        &mut self,
        f: sendspin::audio::AudioFormat,
        c: audio::SharedClock,
        g: audio::Gain,
    ) -> anyhow::Result<()> {
        self.0.begin(f, c, g)
    }
    fn write(&mut self, b: sendspin::audio::AudioBuffer) {
        self.0.write(b);
        std::thread::sleep(Duration::from_millis(40));
    }
    fn clear(&mut self) {
        self.0.clear();
    }
    fn gain(&mut self, g: audio::Gain) {
        self.0.gain(g);
    }
    fn failed(&self) -> bool {
        false
    }
}
#[tokio::test]
async fn stream_end_invalidates_audio_already_queued_to_worker() {
    use tokio_tungstenite::tungstenite::Message as Ws;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let output = events.clone();
    let observations = events.clone();
    let fixture = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        fixture_json(&mut ws).await;
        ws.send(Ws::text(r#"{"type":"auth_ok"}"#)).await.unwrap();
        fixture_json(&mut ws).await;
        ws.send(Ws::text(r#"{"type":"server/hello","payload":{"server_id":"fixture","name":"Fixture","version":1,"active_roles":["player@v1"],"connection_reason":"playback"}}"#)).await.unwrap();
        ws.send(Ws::text(r#"{"type":"stream/start","payload":{"player":{"codec":"pcm","channels":2,"sample_rate":48000,"bit_depth":16}}}"#)).await.unwrap();
        while !observations
            .lock()
            .unwrap()
            .iter()
            .any(|x| x.starts_with("begin:"))
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        for _ in 0..20 {
            let mut chunk = vec![4];
            chunk.extend_from_slice(&0_i64.to_be_bytes());
            chunk.extend_from_slice(&[0, 0, 0, 0]);
            ws.send(Ws::Binary(chunk.into())).await.unwrap();
        }
        while !observations
            .lock()
            .unwrap()
            .iter()
            .any(|x| x.starts_with("samples:"))
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        ws.send(Ws::text(
            r#"{"type":"stream/end","payload":{"roles":["player"]}}"#,
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let log = observations.lock().unwrap();
        assert!(
            log.iter().filter(|x| x.starts_with("samples:")).count() <= 3,
            "stale audio drained after stream/end: {log:?}"
        );
        assert_eq!(log.last().unwrap(), "clear");
    });
    let handle =
        audio::start_with_output(config(base), move || Ok(SlowFixture(FixtureOutput(output))))
            .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(3), fixture).await;
    handle.shutdown().await;
    result.unwrap().unwrap();
}

#[tokio::test]
async fn rejected_proxy_auth_reconnects_with_backoff_and_shutdown_cancels_wait() {
    use tokio_tungstenite::tungstenite::Message as Ws;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (reached, wait) = tokio::sync::oneshot::channel();
    let fixture = tokio::spawn(async move {
        let mut times = Vec::new();
        for attempt in 0..3 {
            let (tcp, _) = listener.accept().await.unwrap();
            times.push(std::time::Instant::now());
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let auth = fixture_json(&mut ws).await;
            assert_eq!(auth["type"], "auth");
            assert_eq!(auth["client_id"], "fixture-id");
            if attempt < 2 {
                ws.send(Ws::text(
                    r#"{"type":"auth_error","detail":"fixture-secret private server response"}"#,
                ))
                .await
                .unwrap();
                // No Sendspin hello may be sent on this rejected socket.
                while let Some(message) = ws.next().await {
                    assert!(!matches!(message, Ok(Ws::Text(_))));
                    if message.is_err() {
                        break;
                    }
                }
            } else {
                reached.send(times).unwrap();
                assert!(
                    tokio::time::timeout(Duration::from_secs(1), ws.next())
                        .await
                        .is_ok(),
                    "shutdown did not close authentication wait"
                );
                assert!(
                    tokio::time::timeout(Duration::from_millis(400), listener.accept())
                        .await
                        .is_err(),
                    "reconnected after shutdown"
                );
                return;
            }
        }
    });
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let handle = audio::start_with_output(config(base), move || Ok(FixtureOutput(events))).unwrap();
    let times = tokio::time::timeout(Duration::from_secs(3), wait)
        .await
        .unwrap()
        .unwrap();
    assert!(times[1].duration_since(times[0]) >= Duration::from_millis(200));
    assert!(times[2].duration_since(times[1]) >= Duration::from_millis(400));
    assert!(!handle.status.borrow().detail.contains("fixture-secret"));
    assert!(!handle
        .status
        .borrow()
        .detail
        .contains("private server response"));
    tokio::time::timeout(Duration::from_secs(1), handle.shutdown())
        .await
        .unwrap();
    fixture.await.unwrap();
}
#[tokio::test]
async fn auth_timeout_and_peer_errors_are_sanitized() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = audio::proxy_url(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let fixture = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        fixture_json(&mut ws).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    let error = audio::authenticate(
        &url,
        "fixture-secret",
        "fixture-id",
        Duration::from_millis(40),
    )
    .await
    .err()
    .unwrap()
    .to_string();
    assert_eq!(error, "Audio proxy authentication timed out");
    fixture.await.unwrap();
}
#[test]
fn invalid_configuration_and_missing_runtime_are_rejected_before_worker_creation() {
    for volume in [101, 255] {
        let mut c = config("http://offline.invalid".into());
        c.volume = volume;
        assert!(audio::start_with_output::<FixtureOutput, _>(c, || panic!(
            "must not create output"
        ))
        .is_err());
    }
    assert!(audio::start_with_output::<FixtureOutput, _>(
        config("http://offline.invalid".into()),
        || panic!("must not create output")
    )
    .is_err());
}

#[test]
fn queue_budget_accounts_for_static_delay_and_rejects_overlap_and_floods() {
    let now = std::time::Instant::now();
    let mut budget = audio::QueueBudget::default();
    // 5s external amplifier latency: a timestamp 5.5s ahead emits in 500ms.
    assert!(budget.accept(
        now,
        (5_500_000, now + Duration::from_millis(5500)),
        Duration::from_millis(20),
        5000,
        3840
    ));
    assert!(!budget.accept(
        now,
        (5_500_000, now + Duration::from_millis(5500)),
        Duration::from_millis(20),
        5000,
        3840
    ));
    let mut budget = audio::QueueBudget::default();
    assert!(!budget.accept(
        now,
        (120_000_000, now + Duration::from_secs(120)),
        Duration::from_millis(20),
        0,
        3840
    ));
    assert!(!budget.accept(
        now,
        (500_000, now + Duration::from_millis(500)),
        Duration::from_millis(20),
        0,
        3 * 1024 * 1024
    ));
}

#[test]
fn queue_accepts_the_pcm_buffer_capacity_advertised_to_music_assistant() {
    let now = std::time::Instant::now();
    let mut budget = audio::QueueBudget::default();
    // MA accounts encoded bytes. 2 MiB of 48 kHz stereo PCM16 spans almost
    // 11 seconds and expands to almost 4 MiB in the i32 output queue.
    for index in 0..540 {
        let offset = Duration::from_millis(500 + index * 20);
        assert!(
            budget.accept(
                now,
                (offset.as_micros() as i64, now + offset),
                Duration::from_millis(20),
                0,
                1920 * 4
            ),
            "legal advertised buffer rejected at chunk {index}"
        );
    }
}

#[test]
fn compressed_audio_horizon_fits_but_decoded_memory_remains_bounded() {
    let now = std::time::Instant::now();
    let mut budget = audio::QueueBudget::default();
    // Worst supported format: 30s stereo 96kHz decoded into i32 samples.
    for index in 0..1500 {
        let offset = Duration::from_millis(500 + index * 20);
        assert!(budget.accept(
            now,
            (offset.as_micros() as i64, now + offset),
            Duration::from_millis(20),
            0,
            3840 * 4
        ));
    }
    let mut budget = audio::QueueBudget::default();
    // Artificially huge chunks at tiny timestamp increments must still fail
    // the aggregate memory bound, even inside the permitted horizon.
    for index in 0..32 {
        let offset = Duration::from_millis(500 + index * 20);
        assert!(budget.accept(
            now,
            (offset.as_micros() as i64, now + offset),
            Duration::from_millis(20),
            0,
            1024 * 1024
        ));
    }
    assert!(!budget.accept(
        now,
        (2_000_000, now + Duration::from_secs(2)),
        Duration::from_millis(20),
        0,
        1024 * 1024
    ));
}

#[tokio::test]
async fn decoder_failure_reason_survives_worker_shutdown_without_peer_data() {
    use tokio_tungstenite::tungstenite::Message as Ws;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        fixture_json(&mut ws).await;
        ws.send(Ws::text(r#"{"type":"auth_ok"}"#)).await.unwrap();
        fixture_json(&mut ws).await;
        ws.send(Ws::text(r#"{"type":"server/hello","payload":{"server_id":"fixture","name":"Fixture","version":1,"active_roles":["player@v1"],"connection_reason":"playback"}}"#)).await.unwrap();
        ws.send(Ws::text(r#"{"type":"stream/start","payload":{"player":{"codec":"private-peer-value","channels":2,"sample_rate":48000,"bit_depth":16}}}"#)).await.unwrap();
        while let Some(Ok(_)) = ws.next().await {}
    });
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut handle =
        audio::start_with_output(config(base), move || Ok(FixtureOutput(events))).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while handle.status.changed().await.is_ok() {}
    })
    .await
    .unwrap();
    let state = handle.status.borrow().clone();
    assert_eq!(state.state, "failed");
    assert_eq!(state.detail, "Unsupported audio stream format");
    handle.shutdown().await;
    server.await.unwrap();
}

struct FaultFixture(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl audio::Output for FaultFixture {
    fn formats(&self) -> anyhow::Result<Vec<sendspin::protocol::messages::AudioFormatSpec>> {
        Ok(audio::formats_for_ranges(&[(2, 48000, 48000)]))
    }
    fn begin(
        &mut self,
        _: sendspin::audio::AudioFormat,
        _: audio::SharedClock,
        _: audio::Gain,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    fn write(&mut self, _: sendspin::audio::AudioBuffer) {}
    fn clear(&mut self) {}
    fn gain(&mut self, _: audio::Gain) {}
    fn failed(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Acquire)
    }
}
#[tokio::test]
async fn worker_error_cancels_in_progress_proxy_handshake() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let output = failed.clone();
    let handle = audio::start_with_output(config(base), move || Ok(FaultFixture(output))).unwrap();
    let (tcp, _) = listener.accept().await.unwrap();
    let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
    fixture_json(&mut ws).await;
    failed.store(true, std::sync::atomic::Ordering::Release);
    let disconnected = tokio::time::timeout(Duration::from_millis(300), ws.next()).await;
    assert!(
        disconnected.is_ok(),
        "failed output left proxy handshake running"
    );
    assert_eq!(handle.status.borrow().state, "failed");
    handle.shutdown().await;
}

#[test]
fn contiguous_server_chunks_survive_earlier_clock_mapping() {
    let now = std::time::Instant::now();
    let mut budget = audio::QueueBudget::default();
    assert!(budget.accept(
        now,
        (500_000, now + Duration::from_millis(500)),
        Duration::from_millis(20),
        0,
        3840
    ));
    // Server timestamps advance exactly 20ms; the next clock estimate moves
    // the converted local timestamp 100us earlier, not the server audio.
    assert!(budget.accept(
        now,
        (520_000, now + Duration::from_micros(519_900)),
        Duration::from_millis(20),
        0,
        3840
    ));
    assert!(!budget.accept(
        now,
        (520_000, now + Duration::from_micros(519_900)),
        Duration::from_millis(20),
        0,
        3840
    ));
}

#[test]
fn queue_budget_expires_reordered_local_deadlines_without_losing_bounds() {
    let now = std::time::Instant::now();
    let mut budget = audio::QueueBudget::default();
    let duration = Duration::from_micros(10);
    // Fill 30 MiB of the 32 MiB bound before the two reordered entries.
    for n in 0..15 {
        assert!(budget.accept(
            now,
            (-150 + n * 10, now + Duration::from_secs(1)),
            duration,
            0,
            2 * 1024 * 1024
        ));
    }
    // A 100us clock shift moves the second deadline before the first.
    assert!(budget.accept(
        now,
        (0, now + Duration::from_micros(100)),
        duration,
        0,
        1024 * 1024
    ));
    assert!(budget.accept(
        now,
        (10, now + Duration::from_micros(10)),
        duration,
        0,
        1024 * 1024
    ));
    assert!(
        !budget.accept(now, (20, now + Duration::from_micros(20)), duration, 0, 1),
        "live byte bound"
    );
    assert!(
        budget.accept(
            now + Duration::from_micros(30),
            (20, now + Duration::from_micros(40)),
            duration,
            0,
            1024 * 1024
        ),
        "expired second entry must not stay behind the live first entry"
    );
    assert!(
        !budget.accept(
            now + Duration::from_secs(1),
            (25, now + Duration::from_secs(1)),
            duration,
            0,
            1
        ),
        "real server overlap despite later local mapping"
    );
    let mut budget = audio::QueueBudget::default();
    for n in 0..4096 {
        assert!(budget.accept(
            now,
            (n * 10, now + Duration::from_micros(100 + n as u64 * 10)),
            duration,
            0,
            1
        ));
    }
    assert!(
        !budget.accept(
            now,
            (40960, now + Duration::from_micros(41060)),
            duration,
            0,
            1
        ),
        "chunk count bound"
    );
}

#[test]
fn delay_change_reanchors_queue_budget() {
    let now = std::time::Instant::now();
    let mut budget = audio::QueueBudget::default();
    assert!(budget.accept(
        now,
        (500_000, now + Duration::from_millis(500)),
        Duration::from_millis(20),
        0,
        3840
    ));
    assert!(budget.set_delay(100));
    assert!(!budget.set_delay(100));
    assert!(budget.accept(
        now,
        (520_000, now + Duration::from_millis(520)),
        Duration::from_millis(20),
        100,
        3840
    ));
}

#[test]
fn proxy_url_preserves_prefix_and_rejects_credentials() {
    assert_eq!(
        audio::proxy_url("https://example.test/music/")
            .unwrap()
            .as_str(),
        "wss://example.test/music/sendspin"
    );
    assert_eq!(
        audio::proxy_url("http://localhost:8095").unwrap().as_str(),
        "ws://localhost:8095/sendspin"
    );
    for bad in [
        "ftp://example.test",
        "https://user:secret@example.test",
        "http://example.test/?token=secret",
        "http://example.test/#secret",
    ] {
        assert!(audio::proxy_url(bad).is_err());
    }
}
