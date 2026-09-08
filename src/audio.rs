//! Embedded, authenticated Music Assistant Sendspin player.
//!
//! Compatibility source: music-assistant/server tag 2.10.2,
//! controllers/webserver/controller.py (`GET /sendspin`) and
//! controllers/webserver/sendspin_proxy.py (`auth` then `auth_ok`).
//!
//! App-owned handoffs are bounded and never wait for the output thread. The
//! pinned Sendspin 0.3.7 router itself uses unbounded receivers, and SyncedPlayer
//! takes internal queue locks in its callback: this is not a hard-real-time or
//! globally lock-free guarantee. Its split receivers also lack stream sequence
//! IDs, so boundary handling deliberately discards queued audio for safety.
//! Do not enable upstream Sendspin payload logging when installing a logger;
//! all errors/status emitted by this module are intentionally sanitized.
use anyhow::{bail, Result};
use base64::Engine;
use sendspin::audio::decode::{Decoder, FlacDecoder, OpusDecoder, PcmDecoder, PcmEndian};
use sendspin::audio::{AudioFormat, Codec};
use sendspin::protocol::messages::{AudioFormatSpec, StreamPlayerConfig};

pub(crate) struct StreamDecoder {
    format: AudioFormat,
    decoder: Box<dyn Decoder>,
}
impl StreamDecoder {
    pub(crate) fn new(config: &StreamPlayerConfig, supported: &[AudioFormatSpec]) -> Result<Self> {
        if !supported.iter().any(|s| {
            s.codec == config.codec
                && s.channels == config.channels
                && s.sample_rate == config.sample_rate
                && s.bit_depth == config.bit_depth
        }) || !matches!(config.channels, 1 | 2)
            || !matches!(config.bit_depth, 16 | 24)
            || config.sample_rate == 0
        {
            bail!("Unsupported audio stream format");
        }
        let header = config
            .codec_header
            .as_ref()
            .map(|h| {
                base64::prelude::BASE64_STANDARD
                    .decode(h)
                    .map_err(|_| anyhow::anyhow!("Invalid audio codec header"))
            })
            .transpose()?;
        if config.codec == "flac" {
            if let Some(h) = &header {
                if h.len() < 42 || &h[..4] != b"fLaC" || h[4] & 0x7f != 0 || h[5..8] != [0, 0, 34] {
                    bail!("Invalid FLAC stream information");
                }
                let packed = u64::from_be_bytes(
                    h[18..26]
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("Invalid FLAC stream information"))?,
                );
                if (packed >> 44) as u32 != config.sample_rate
                    || ((packed >> 41) & 7) as u8 + 1 != config.channels
                    || ((packed >> 36) & 31) as u8 + 1 != config.bit_depth
                {
                    bail!("FLAC stream information disagrees with negotiation");
                }
            }
        }
        let (codec, decoder): (_, Box<dyn Decoder>) = match config.codec.as_str() {
            "pcm" => (
                Codec::Pcm,
                Box::new(PcmDecoder::with_endian(config.bit_depth, PcmEndian::Little)),
            ),
            "flac" => (
                Codec::Flac,
                Box::new(match &header {
                    Some(h) => FlacDecoder::with_header(h)
                        .map_err(|_| anyhow::anyhow!("Invalid FLAC header"))?,
                    None => FlacDecoder::new(),
                }),
            ),
            "opus" => (
                Codec::Opus,
                Box::new(
                    OpusDecoder::new(config.sample_rate, config.channels)
                        .map_err(|_| anyhow::anyhow!("Invalid Opus format"))?,
                ),
            ),
            _ => bail!("Unsupported audio codec"),
        };
        Ok(Self {
            format: AudioFormat {
                codec,
                channels: config.channels,
                sample_rate: config.sample_rate,
                bit_depth: config.bit_depth,
                codec_header: header,
            },
            decoder,
        })
    }
    pub(crate) fn decode(&mut self, data: &[u8]) -> Result<std::sync::Arc<[i32]>> {
        let frame = self.format.channels as usize * (self.format.bit_depth as usize / 8);
        if data.len() > 1024 * 1024
            || (self.format.codec == Codec::Pcm && !data.len().is_multiple_of(frame))
        {
            bail!("Invalid audio frame size");
        }
        let samples = self
            .decoder
            .decode(data)
            .map_err(|_| anyhow::anyhow!("Audio decoding failed"))?;
        if samples.len() % self.format.channels as usize != 0
            || samples.len() > self.format.sample_rate as usize * self.format.channels as usize * 2
        {
            bail!("Invalid decoded audio size");
        }
        Ok(samples)
    }
}

use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio_tungstenite::{tungstenite::Message as WsMessage, MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Authentication errors deliberately contain neither credentials nor peer input.
pub(crate) async fn authenticate(
    url: &url::Url,
    token: &str,
    id: &str,
    deadline: Duration,
) -> Result<Socket> {
    tokio::time::timeout(deadline, async {
        let (mut socket, _) = tokio_tungstenite::connect_async_with_config(
            url.as_str(),
            Some(
                tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
                    .max_message_size(Some(1024 * 1024))
                    .max_frame_size(Some(1024 * 1024)),
            ),
            false,
        )
        .await
        .map_err(|_| anyhow::anyhow!("Audio proxy connection failed"))?;
        socket
            .send(WsMessage::text(
                serde_json::json!({"type":"auth", "token":token, "client_id":id}).to_string(),
            ))
            .await
            .map_err(|_| anyhow::anyhow!("Audio proxy authentication send failed"))?;
        loop {
            match socket.next().await {
                Some(Ok(WsMessage::Text(text))) => {
                    let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
                        anyhow::anyhow!("Invalid audio proxy authentication response")
                    })?;
                    if value.get("type").and_then(|v| v.as_str()) == Some("auth_ok") {
                        return Ok(socket);
                    }
                    bail!("Audio proxy authentication rejected");
                }
                Some(Ok(WsMessage::Ping(data))) => socket
                    .send(WsMessage::Pong(data))
                    .await
                    .map_err(|_| anyhow::anyhow!("Audio proxy disconnected"))?,
                _ => bail!("Audio proxy authentication failed"),
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Audio proxy authentication timed out"))?
}

use sendspin::audio::AudioBuffer;
use sendspin::protocol::messages::{
    ClientState, Message, PlayerCommandType, PlayerState, PlayerStateCommand, PlayerV1Support,
};
use sendspin::ProtocolClientBuilder;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc as thread_channel, Arc,
};
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Clone, Debug, Default)]
pub struct AudioStatus {
    pub state: String,
    pub detail: String,
}
// Deliberately no Debug implementation: this contains a credential.
pub struct AudioConfig {
    pub server: String,
    pub token: String,
    pub player_id: String,
    pub player_name: String,
    pub device_id: Option<String>,
    pub volume: u8,
    pub muted: bool,
}
pub struct AudioHandle {
    pub status: watch::Receiver<AudioStatus>,
    cancel: watch::Sender<bool>,
    stop: Arc<AtomicBool>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl AudioHandle {
    pub async fn shutdown(mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.cancel.send(true);
        if let Some(mut task) = self.task.take() {
            if tokio::time::timeout(Duration::from_secs(3), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }
}
impl Drop for AudioHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.cancel.send(true);
    }
}
pub(crate) type SharedClock = Arc<parking_lot::Mutex<sendspin::sync::ClockSync>>;
#[derive(Clone, Copy)]
pub(crate) struct Gain {
    pub volume: u8,
    pub muted: bool,
    pub delay: u16,
}
impl Gain {
    fn state(self) -> PlayerState {
        PlayerState {
            volume: Some(self.volume),
            muted: Some(self.muted),
            static_delay_ms: Some(self.delay),
            required_lead_time_ms: Some(500),
            min_buffer_ms: Some(500),
            supported_commands: Some(vec![PlayerStateCommand::SetStaticDelay]),
        }
    }
}
/// Receives decoded samples that are about to be played, with the local
/// instant the output is scheduled to emit them. Declared here so the audio
/// module does not depend on the interface; only real device output has one.
pub trait SampleSink: Send + Sync + 'static {
    fn push(&self, samples: &[i32], channels: u8, rate: u32, emitted: std::time::Instant);
    fn set_muted(&self, muted: bool);
    fn clear(&self);
}

// Constructed and destroyed on the audio thread; no Send requirement on CPAL.
pub(crate) trait Output: 'static {
    fn formats(&self) -> Result<Vec<AudioFormatSpec>>;
    fn begin(&mut self, format: AudioFormat, clock: SharedClock, gain: Gain) -> Result<()>;
    fn write(&mut self, buffer: AudioBuffer);
    fn clear(&mut self);
    fn gain(&mut self, gain: Gain);
    fn failed(&self) -> bool;
    fn failure_detail(&self) -> &'static str {
        "Audio output failed"
    }
}
enum Work {
    Begin(StreamPlayerConfig, SharedClock, Gain, bool),
    Audio(sendspin::protocol::client::AudioChunk),
    End,
    Gain(Gain),
}
enum Feedback {
    Gain(Gain),
    Failed,
}
fn status(tx: &watch::Sender<AudioStatus>, state: &str, detail: &str) {
    tx.send_replace(AudioStatus {
        state: state.into(),
        detail: detail.into(),
    });
}

// Only app-owned fixed messages may reach the TUI; decoder/driver errors can
// otherwise contain arbitrary peer data or local configuration.
fn safe_audio_error(error: &anyhow::Error) -> &'static str {
    let message = error.to_string();
    for safe in [
        "Unsupported audio stream format",
        "Invalid audio codec header",
        "Invalid FLAC stream information",
        "FLAC stream information disagrees with negotiation",
        "Invalid FLAC header",
        "Invalid Opus format",
        "Unsupported audio codec",
        "Invalid audio frame size",
        "Audio decoding failed",
        "Invalid decoded audio size",
        "Audio output stream creation failed",
        "Selected audio output device not found",
        "No default audio output device",
        "Audio output configuration unavailable",
        "Audio output formats unavailable",
        "No supported audio output formats",
        "Audio command queue overflow",
    ] {
        if message == safe {
            return safe;
        }
    }
    "Audio output or decoding failed"
}

fn worker_stopped(tx: &watch::Sender<AudioStatus>) {
    tx.send_if_modified(|current| {
        if current.state == "failed" {
            return false;
        }
        *current = AudioStatus {
            state: "failed".into(),
            detail: "Audio worker stopped".into(),
        };
        true
    });
}

pub(crate) fn start_with_output<O, F>(config: AudioConfig, factory: F) -> Result<AudioHandle>
where
    O: Output,
    F: FnOnce() -> Result<O> + Send + 'static,
{
    let url = proxy_url(&config.server)?;
    if config.token.trim().is_empty()
        || config.player_id.trim().is_empty()
        || config.player_name.trim().is_empty()
        || config.volume > 100
    {
        bail!("Invalid audio configuration");
    }
    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|_| anyhow::anyhow!("Audio requires a Tokio runtime"))?;
    let (status_tx, status_rx) = watch::channel(AudioStatus {
        state: "starting".into(),
        detail: "Initializing audio output".into(),
    });
    let (cancel, mut cancellation) = watch::channel(false);
    let stop = Arc::new(AtomicBool::new(false));
    let epoch = Arc::new(AtomicU64::new(0));
    let (work_tx, work_rx) = thread_channel::sync_channel(64);
    let (feedback_tx, mut feedback_rx) = mpsc::channel(32);
    let (ready_tx, ready_rx) = oneshot::channel();
    let (done_tx, mut done_rx) = oneshot::channel();
    let worker_stop = stop.clone();
    let worker_epoch = epoch.clone();
    let worker_status = status_tx.clone();
    let mut gain = Gain {
        volume: config.volume,
        muted: config.muted,
        delay: 0,
    };
    let worker = std::thread::Builder::new()
        .name("local-matui-audio".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut output = match factory() {
                    Ok(o) => o,
                    Err(error) => {
                        let _ = ready_tx.send(Err(safe_audio_error(&error)));
                        return;
                    }
                };
                let formats = match output.formats() {
                    Ok(f) if !f.is_empty() => f,
                    _ => {
                        let _ = ready_tx.send(Err("No supported audio output formats"));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(formats.clone()));
                let mut current_epoch = 0;
                let mut decoder: Option<StreamDecoder> = None;
                let mut setup: Option<(StreamPlayerConfig, SharedClock)> = None;
                let mut active = false;
                let mut failed = None;
                while !worker_stop.load(Ordering::Acquire) {
                    let new_epoch = worker_epoch.load(Ordering::Acquire);
                    if current_epoch != new_epoch {
                        output.clear();
                        decoder = None;
                        setup = None;
                        active = false;
                        current_epoch = new_epoch;
                    }
                    if output.failed() {
                        failed = Some(output.failure_detail());
                        break;
                    }
                    let (generation, event) = match work_rx.recv_timeout(Duration::from_millis(10))
                    {
                        Ok(w) => w,
                        Err(thread_channel::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    };
                    if generation != worker_epoch.load(Ordering::Acquire) {
                        continue;
                    }
                    // The generation may have changed while recv_timeout was waiting.
                    if generation != current_epoch {
                        output.clear();
                        decoder = None;
                        setup = None;
                        active = false;
                        current_epoch = generation;
                    }
                    let result: Result<()> = (|| {
                        match event {
                            Work::Begin(config, clock, next_gain, start_now) => {
                                output.clear();
                                decoder = None;
                                setup = None;
                                active = false;
                                let next = StreamDecoder::new(&config, &formats)?;
                                gain = next_gain;
                                if start_now {
                                    output.begin(next.format.clone(), clock.clone(), gain)?;
                                }
                                decoder = Some(next);
                                setup = Some((config, clock));
                                active = start_now;
                                status(&worker_status, "ready", "Audio stream configured");
                            }
                            Work::Audio(chunk) => {
                                if let Some(ref mut dec) = decoder {
                                    let samples = dec.decode(&chunk.data)?;
                                    if !active {
                                        if let Some((_, clock)) = &setup {
                                            output.begin(
                                                dec.format.clone(),
                                                clock.clone(),
                                                gain,
                                            )?;
                                            active = true;
                                        }
                                    }
                                    // A disconnect/clear can invalidate a decode in flight.
                                    if generation == worker_epoch.load(Ordering::Acquire)
                                        && !worker_stop.load(Ordering::Acquire)
                                    {
                                        output.write(AudioBuffer {
                                            timestamp: chunk.timestamp,
                                            samples,
                                            format: dec.format.clone(),
                                        });
                                    }
                                }
                            }
                            Work::End => {
                                output.clear();
                                decoder = None;
                                setup = None;
                                active = false;
                            }
                            Work::Gain(next) => {
                                gain = next;
                                output.gain(gain);
                                feedback_tx
                                    .try_send((generation, Feedback::Gain(gain)))
                                    .map_err(|_| anyhow::anyhow!("Audio command queue overflow"))?;
                            }
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        failed = Some(safe_audio_error(&error));
                        break;
                    }
                }
                output.clear();
                if let Some(detail) = failed {
                    status(&worker_status, "failed", detail);
                    let _ = feedback_tx.try_send((current_epoch, Feedback::Failed));
                }
            }));
            if result.is_err() {
                status(&worker_status, "failed", "Audio worker failed");
            }
            let _ = done_tx.send(());
        })
        .map_err(|_| anyhow::anyhow!("Unable to start audio worker"))?;
    let task_stop = stop.clone();
    let task = runtime.spawn(async move {
        let mut worker_finished = false;
        let ready = tokio::select! {biased; _=cancellation.changed()=>None, r=ready_rx=>r.ok()};
        if let Some(Ok(formats)) = ready {
            let mut retry = Duration::from_millis(250);
            loop {
                if *cancellation.borrow() || task_stop.load(Ordering::Acquire) {break;}
                status(&status_tx,"connecting","Connecting to authenticated audio proxy");
                let started=std::time::Instant::now();
                let result=tokio::select! {
                    biased;
                    _=cancellation.changed()=>break,
                    _=&mut done_rx=>{worker_finished=true;worker_stopped(&status_tx);break;},
                    r=session(&config,&url,&formats,&mut gain,SessionIo {work:&work_tx,epoch:&epoch,feedback:&mut feedback_rx},&status_tx)=>r,
                };
                epoch.fetch_add(1,Ordering::AcqRel);
                if status_tx.borrow().state=="failed" {break;}
                if feedback_rx.is_closed() {worker_stopped(&status_tx);break;}
                let detail=result.err().map(|e|e.to_string()).unwrap_or_else(||"Audio connection closed".into());
                status(&status_tx,"reconnecting",&detail);
                if started.elapsed() > Duration::from_secs(30) {retry=Duration::from_millis(250);}
                tokio::select! {biased; _=cancellation.changed()=>break, _=&mut done_rx=>{worker_finished=true;worker_stopped(&status_tx);break;}, _=tokio::time::sleep(retry)=>{}}
                retry=(retry*2).min(Duration::from_secs(30));
            }
        } else if !*cancellation.borrow() {status(&status_tx,"failed",ready.and_then(Result::err).unwrap_or("Audio output initialization failed"));}
        task_stop.store(true,Ordering::Release);
        drop(work_tx);
        // Never block a Tokio executor on a device driver. Rust cannot kill a
        // stuck OS thread; timeout detaches it, still owning only its own output.
        if worker_finished || tokio::time::timeout(Duration::from_secs(2),done_rx).await.is_ok() {let _=worker.join();}
        if *cancellation.borrow() {status(&status_tx,"stopped","Audio stopped");}
    });
    Ok(AudioHandle {
        status: status_rx,
        cancel,
        stop,
        task: Some(task),
    })
}

struct SessionIo<'a> {
    work: &'a thread_channel::SyncSender<(u64, Work)>,
    epoch: &'a Arc<AtomicU64>,
    feedback: &'a mut mpsc::Receiver<(u64, Feedback)>,
}

async fn session(
    config: &AudioConfig,
    url: &url::Url,
    formats: &[AudioFormatSpec],
    gain: &mut Gain,
    io: SessionIo<'_>,
    state: &watch::Sender<AudioStatus>,
) -> Result<()> {
    let SessionIo {
        work,
        epoch,
        feedback,
    } = io;
    let socket = authenticate(
        url,
        &config.token,
        &config.player_id,
        Duration::from_secs(10),
    )
    .await?;
    let builder = ProtocolClientBuilder::builder()
        .client_id(config.player_id.clone())
        .name(config.player_name.clone())
        .player_v1_support(PlayerV1Support {
            supported_formats: formats.to_vec(),
            buffer_capacity: 2 * 1024 * 1024,
            supported_commands: vec!["volume".into(), "mute".into()],
        })
        .initial_player_state(gain.state())
        .build();
    let client = tokio::time::timeout(Duration::from_secs(10), builder.accept(socket))
        .await
        .map_err(|_| anyhow::anyhow!("Sendspin handshake timed out"))?
        .map_err(|_| anyhow::anyhow!("Sendspin handshake failed"))?;
    let mut connection = client.split();
    // These roles were not requested. Closing their library receivers prevents
    // an unsolicited artwork/visualizer stream accumulating unread payloads.
    connection.artwork.close();
    connection.visualizer.close();
    while connection.artwork.try_recv().is_ok() {}
    while connection.visualizer.try_recv().is_ok() {}
    status(state, "ready", "Authenticated audio player connected");
    let mut stream_config: Option<StreamPlayerConfig> = None;
    loop {
        // Sendspin 0.3.7 exposes unbounded internal receivers. Drain promptly,
        // cap observed backlog and never await the bounded audio worker queue.
        if connection.audio.len() > 64 || connection.messages.len() > 64 {
            bail!("Audio receive queue overflow");
        }
        let generation = epoch.load(Ordering::Acquire);
        let send = |event| {
            work.try_send((epoch.load(Ordering::Acquire), event))
                .map_err(|_| anyhow::anyhow!("Audio worker queue full or unavailable"))
        };
        tokio::select! {
            biased;
            event=feedback.recv()=>match event {
                Some((g,Feedback::Gain(next))) if g==generation => {
                    connection.sender.send_message(Message::ClientState(ClientState {state:None,player:Some(next.state())})).await.map_err(|_|anyhow::anyhow!("Audio state confirmation failed"))?;
                }
                Some((_,Feedback::Failed))|None=>bail!("Audio output failed"),
                _=>{}
            },
            message=connection.messages.recv()=>match message {
                None=>bail!("Audio proxy disconnected"),
                Some(Message::StreamStart(start))=>if let Some(player)=start.player {
                    // Split protocol receivers do not carry ordering metadata.
                    // Discard queued chunks at a stream boundary rather than
                    // accidentally decode old-format bytes under the new format.
                    while connection.audio.try_recv().is_ok() {}
                    epoch.fetch_add(1,Ordering::AcqRel);
                    stream_config=Some(player.clone());
                    send(Work::Begin(player,connection.clock_sync.clone(),*gain,true))?;
                },
                Some(Message::StreamClear(clear))=>if clear.roles.as_ref().is_none_or(|roles|roles.iter().any(|r|r=="player")) {while connection.audio.try_recv().is_ok(){} epoch.fetch_add(1,Ordering::AcqRel);
                    if let Some(config)=&stream_config {send(Work::Begin(config.clone(),connection.clock_sync.clone(),*gain,false))?;} else {send(Work::End)?;}},
                Some(Message::StreamEnd(end))=>if end.roles.as_ref().is_none_or(|roles|roles.iter().any(|r|r=="player")) {while connection.audio.try_recv().is_ok(){} epoch.fetch_add(1,Ordering::AcqRel); stream_config=None; send(Work::End)?;},
                Some(Message::ServerCommand(command))=>if let Some(command)=command.player {
                    let mut next=*gain;
                    match command.command {
                        PlayerCommandType::Volume=>if let Some(volume)=command.volume {if volume>100 {bail!("Invalid audio volume command");}next.volume=volume;},
                        PlayerCommandType::Mute=>if let Some(muted)=command.mute {next.muted=muted;},
                        PlayerCommandType::SetStaticDelay=>if let Some(delay)=command.static_delay_ms {if delay>5000 {bail!("Invalid audio delay command");}next.delay=delay;},
                        _=>continue,
                    }
                    *gain=next; send(Work::Gain(next))?;
                },
                _=>{}
            },
            chunk=connection.audio.recv()=>match chunk {Some(c)=>send(Work::Audio(c))?,None=>bail!("Audio proxy disconnected")},
        }
    }
}

use cpal::traits::{DeviceTrait, HostTrait};
use sendspin::audio::{SyncedPlayer, SyncedPlayerConfig};

#[derive(Clone, Debug)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}
/// Enumerate locally, without creating a stream or connecting to any server.
pub fn devices() -> Result<Vec<AudioDevice>> {
    Ok(enumerate_devices()?
        .into_iter()
        .map(|(_, info)| info)
        .collect())
}
fn enumerate_devices() -> Result<Vec<(cpal::Device, AudioDevice)>> {
    let mut found = Vec::new();
    for id in cpal::available_hosts() {
        let host = cpal::host_from_id(id).map_err(|_| anyhow::anyhow!("Audio host unavailable"))?;
        for device in host
            .devices()
            .map_err(|_| anyhow::anyhow!("Audio device enumeration failed"))?
        {
            if !device
                .supported_output_configs()
                .is_ok_and(|mut c| c.next().is_some())
            {
                continue;
            }
            let id = device
                .id()
                .map_err(|_| anyhow::anyhow!("Audio device identifier unavailable"))?
                .to_string();
            let name = device
                .description()
                .map(|d| d.to_string())
                .unwrap_or_else(|_| id.clone());
            found.push((device, AudioDevice { id, name }));
        }
    }
    Ok(found)
}
/// `spectrum` receives what this endpoint plays, for the interface visualizer.
pub fn start(config: AudioConfig, spectrum: Option<Arc<dyn SampleSink>>) -> Result<AudioHandle> {
    let device_id = config.device_id.clone();
    start_with_output(config, move || {
        DeviceOutput::new(device_id.as_deref(), spectrum)
    })
}

pub(crate) fn formats_for_ranges(ranges: &[(u16, u32, u32)]) -> Vec<AudioFormatSpec> {
    let mut result = Vec::new();
    // The device output sample representation is independent of wire depth.
    // PCM first avoids compressed-codec compatibility surprises on MA 2.10.2.
    for (codec, bit_depth) in [
        ("pcm", 16),
        ("pcm", 24),
        ("flac", 16),
        ("flac", 24),
        ("opus", 16),
    ] {
        for rate in [48000, 44100, 32000, 24000, 16000, 96000] {
            if codec == "opus" && rate != 48000 {
                continue;
            }
            for channels in [2, 1] {
                if ranges
                    .iter()
                    .any(|&(c, min, max)| c == channels && min <= rate && rate <= max)
                {
                    result.push(AudioFormatSpec {
                        codec: codec.into(),
                        channels: channels as u8,
                        sample_rate: rate,
                        bit_depth,
                    });
                }
            }
        }
    }
    result
}

#[derive(Default)]
pub(crate) struct QueueBudget {
    pending: std::collections::VecDeque<(std::time::Instant, usize)>,
    // Server microseconds, independent of mutable clock-sync estimates.
    last_end: Option<i128>,
    delay: u16,
    error: Option<&'static str>,
}

// MA 2.10.2's aiosendspin 9.1.1 accounts ENCODED bytes and permits a 30s
// buffered horizon. Our i32 queue needs up to 30 * 96000 * 2 * 4 bytes,
// independently of the 2 MiB encoded capacity advertised in client/hello.
const MAX_DECODED_QUEUE_BYTES: usize = 32 * 1024 * 1024;
const MAX_QUEUE_HORIZON: Duration = Duration::from_secs(35);
const MAX_QUEUED_CHUNKS: usize = 4096;
impl QueueBudget {
    pub(crate) fn set_delay(&mut self, delay: u16) -> bool {
        if self.delay == delay {
            return false;
        }
        *self = Self {
            delay,
            ..Default::default()
        };
        true
    }
    pub(crate) fn accept(
        &mut self,
        now: std::time::Instant,
        (server_timestamp, timestamp): (i64, std::time::Instant),
        duration: Duration,
        delay: u16,
        bytes: usize,
    ) -> bool {
        self.set_delay(delay);
        let Some(when) = timestamp.checked_sub(Duration::from_millis(delay.into())) else {
            return false;
        };
        let Some(end) = when.checked_add(duration) else {
            return false;
        };
        // Clock estimate updates can reorder local deadlines. Expire every
        // completed entry, not only a presumed time-ordered prefix.
        self.pending.retain(|(end, _)| *end > now);
        // A 2us tolerance accommodates integer timestamp rounding between chunks.
        self.error = if duration > Duration::from_secs(2) || bytes > 2 * 1024 * 1024 {
            Some("Audio chunk exceeds supported size")
        } else if when > now + MAX_QUEUE_HORIZON {
            Some("Audio timestamp exceeds supported buffer horizon")
        } else if self
            .last_end
            .is_some_and(|previous| i128::from(server_timestamp) + 2 < previous)
        {
            Some("Audio chunks have overlapping timestamps")
        } else if self.pending.iter().map(|(_, n)| *n).sum::<usize>() + bytes
            > MAX_DECODED_QUEUE_BYTES
            || self.pending.len() >= MAX_QUEUED_CHUNKS
        {
            Some("Decoded audio buffer capacity exceeded")
        } else {
            None
        };
        if self.error.is_some() {
            return false;
        }
        self.last_end = Some(i128::from(server_timestamp) + duration.as_micros() as i128);
        self.pending.push_back((end, bytes));
        true
    }
}

struct DeviceOutput {
    device: cpal::Device,
    // Decoded samples are copied here for the visualizer, tagged with the
    // instant the synchronized player is scheduled to emit them.
    spectrum: Option<Arc<dyn SampleSink>>,
    formats: Vec<AudioFormatSpec>,
    player: Option<SyncedPlayer>,
    clock: Option<SharedClock>,
    // Library's decoded queue is unbounded. Track scheduled buffers ourselves,
    // rejecting excessive or overlapping timestamps before enqueueing.
    queued: QueueBudget,
    failed: Option<&'static str>,
}
impl DeviceOutput {
    fn new(id: Option<&str>, spectrum: Option<Arc<dyn SampleSink>>) -> Result<Self> {
        let device = if let Some(id) = id {
            enumerate_devices()?
                .into_iter()
                .find(|(_, info)| info.id == id)
                .map(|(d, _)| d)
                .ok_or_else(|| anyhow::anyhow!("Selected audio output device not found"))?
        } else {
            cpal::default_host()
                .default_output_device()
                .ok_or_else(|| anyhow::anyhow!("No default audio output device"))?
        };
        // 0.3.7 builds using the default output sample type, overriding only
        // channels and rate. Advertise ranges supporting exactly that type.
        let sample_type = device
            .default_output_config()
            .map_err(|_| anyhow::anyhow!("Audio output configuration unavailable"))?
            .sample_format();
        let ranges = device
            .supported_output_configs()
            .map_err(|_| anyhow::anyhow!("Audio output formats unavailable"))?
            .filter(|r| r.sample_format() == sample_type)
            .map(|r| (r.channels(), r.min_sample_rate(), r.max_sample_rate()))
            .collect::<Vec<_>>();
        let formats = formats_for_ranges(&ranges);
        if formats.is_empty() {
            bail!("No supported audio output formats");
        }
        Ok(Self {
            device,
            spectrum,
            formats,
            player: None,
            clock: None,
            queued: Default::default(),
            failed: None,
        })
    }
}
impl Output for DeviceOutput {
    fn formats(&self) -> Result<Vec<AudioFormatSpec>> {
        Ok(self.formats.clone())
    }
    fn begin(&mut self, format: AudioFormat, clock: SharedClock, gain: Gain) -> Result<()> {
        self.clear();
        let player = SyncedPlayer::new(
            format,
            clock.clone(),
            SyncedPlayerConfig {
                device: Some(self.device.clone()),
                volume: gain.volume,
                muted: gain.muted,
                buffer_size: None,
            },
        )
        .map_err(|_| anyhow::anyhow!("Audio output stream creation failed"))?;
        player.set_static_delay(gain.delay);
        self.player = Some(player);
        self.clock = Some(clock);
        if let Some(spectrum) = &self.spectrum {
            spectrum.set_muted(gain.muted);
        }
        Ok(())
    }
    fn write(&mut self, buffer: AudioBuffer) {
        let (Some(player), Some(clock)) = (&self.player, &self.clock) else {
            return;
        };
        let sync = clock.lock();
        // No free-running output: wait for the library's monotonic clock sync.
        // Never add buffers to its unbounded queue while time is unknown/stale.
        if !sync.is_synchronized() || sync.is_stale() {
            drop(sync);
            player.clear();
            self.queued = QueueBudget::default();
            return;
        }
        let Some(when) = sync.server_to_local_instant(buffer.timestamp) else {
            return;
        };
        drop(sync);
        let now = std::time::Instant::now();
        let duration = Duration::from_micros(buffer.duration_us().max(0) as u64);
        let bytes = buffer.samples.len() * std::mem::size_of::<i32>();
        if !self.queued.accept(
            now,
            (buffer.timestamp, when),
            duration,
            player.static_delay_ms(),
            bytes,
        ) {
            self.failed = Some(self.queued.error.unwrap_or("Invalid audio timestamp"));
            player.clear();
            return;
        }
        // Late chunks cannot contribute to audible output and need not queue.
        if when + duration < now {
            return;
        }
        // Sendspin emits each sample `static_delay_ms` early so downstream
        // latency lands it on time, and the budget above measures the same
        // instant. Analyze against that scheduled emission; it is not a
        // measurement of device buffering or acoustic output.
        if let (Some(spectrum), Some(emitted)) = (
            &self.spectrum,
            when.checked_sub(Duration::from_millis(player.static_delay_ms().into())),
        ) {
            spectrum.push(
                &buffer.samples,
                buffer.format.channels,
                buffer.format.sample_rate,
                emitted,
            );
        }
        player.enqueue(buffer);
    }
    fn clear(&mut self) {
        if let Some(player) = self.player.take() {
            player.clear();
            drop(player);
        }
        self.clock = None;
        self.queued = QueueBudget::default();
        if let Some(spectrum) = &self.spectrum {
            spectrum.clear();
        }
    }
    fn gain(&mut self, gain: Gain) {
        if let Some(spectrum) = &self.spectrum {
            spectrum.set_muted(gain.muted);
        }
        if let Some(player) = &self.player {
            player.set_volume(gain.volume);
            player.set_mute(gain.muted);
            if player.static_delay_ms() != gain.delay {
                // The budget may be fresh after begin or a stale-clock reset;
                // only the actual player tells us whether its delay changed.
                player.clear();
                player.set_static_delay(gain.delay);
                self.queued = QueueBudget::default();
                // A delay change moves every scheduled emission instant.
                if let Some(spectrum) = &self.spectrum {
                    spectrum.clear();
                }
            }
            self.queued.set_delay(player.static_delay_ms());
        }
    }
    fn failed(&self) -> bool {
        self.failed.is_some() || self.player.as_ref().is_some_and(|p| p.has_error())
    }
    fn failure_detail(&self) -> &'static str {
        self.failed
            .unwrap_or("Audio output device reported a stream error")
    }
}

/// MA 2.10.2 mounts the authenticated receiver at /sendspin.
pub(crate) fn proxy_url(base: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(base).map_err(|_| anyhow::anyhow!("Invalid audio server URL"))?;
    let scheme = match url.scheme() {
        "http" => "ws",
        "https" => "wss",
        _ => bail!("Audio server requires HTTP or HTTPS"),
    };
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("Audio server URL must not contain credentials, query, or fragment");
    }
    url.set_scheme(scheme)
        .map_err(|_| anyhow::anyhow!("Invalid audio server URL"))?;
    let path = format!("{}/sendspin", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

#[cfg(test)]
mod device_output_tests {
    use super::*;

    /// Records what the output hands to the visualizer, without depending on
    /// the analyzer itself.
    #[derive(Default)]
    struct RecordingSink {
        chunks: parking_lot::Mutex<Vec<(usize, u8, u32, std::time::Instant)>>,
        cleared: AtomicU64,
    }
    impl SampleSink for RecordingSink {
        fn push(&self, samples: &[i32], channels: u8, rate: u32, emitted: std::time::Instant) {
            self.chunks
                .lock()
                .push((samples.len(), channels, rate, emitted));
        }
        fn set_muted(&self, _muted: bool) {}
        fn clear(&self) {
            self.cleared.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[test]
    #[ignore = "requires Linux ALSA null output; run explicitly"]
    fn synchronized_output_accepts_full_advertised_pcm_buffer() {
        let sink = Arc::new(RecordingSink::default());
        let mut output = DeviceOutput::new(Some("alsa:null"), Some(sink.clone())).unwrap();
        let mut sync = sendspin::sync::ClockSync::default();
        std::thread::sleep(Duration::from_millis(2));
        let now = sync.clock().now_micros();
        sync.update(now - 500, now - 450, now - 450, now - 400);
        sync.update(now - 100, now - 50, now - 50, now);
        assert!(sync.is_synchronized());
        let start = sync.client_to_server_micros(now).unwrap() + 500_000;
        let clock = Arc::new(parking_lot::Mutex::new(sync));
        let format = AudioFormat {
            codec: Codec::Pcm,
            sample_rate: 48000,
            channels: 2,
            bit_depth: 16,
            codec_header: None,
        };
        output
            .begin(
                format.clone(),
                clock,
                Gain {
                    volume: 0,
                    muted: true,
                    delay: 0,
                },
            )
            .unwrap();
        for index in 0..540 {
            output.write(AudioBuffer {
                timestamp: start + index * 20_000,
                samples: vec![0; 1920].into(),
                format: format.clone(),
            });
            assert!(
                !output.failed(),
                "{} at chunk {index}",
                output.failure_detail()
            );
        }
        assert!(
            output.queued.pending.len() > 500,
            "buffers must pass through real synchronized output"
        );
        // Every buffer accepted for playback reaches the visualizer, tagged
        // with the instant the player is scheduled to emit it.
        let chunks = sink.chunks.lock().clone();
        assert_eq!(chunks.len(), 540);
        assert!(chunks
            .iter()
            .all(|(samples, channels, rate, _)| *samples == 1920
                && *channels == 2
                && *rate == 48000));
        for pair in chunks.windows(2) {
            let step = pair[1].3.duration_since(pair[0].3);
            assert!(
                step.abs_diff(Duration::from_millis(20)) < Duration::from_millis(1),
                "scheduled emission must advance with the audio: {step:?}"
            );
        }
        output.clear();
        assert!(sink.cleared.load(Ordering::Acquire) > 0);
    }

    #[test]
    #[ignore = "requires Linux ALSA null output; run explicitly"]
    fn begin_delay_can_reset_to_zero_before_audio_or_after_clock_reset() {
        let mut output = DeviceOutput::new(Some("alsa:null"), None).unwrap();
        let clock =
            std::sync::Arc::new(parking_lot::Mutex::new(sendspin::sync::ClockSync::default()));
        let format = AudioFormat {
            codec: Codec::Pcm,
            sample_rate: 48000,
            channels: 2,
            bit_depth: 16,
            codec_header: None,
        };
        let mut gain = Gain {
            volume: 30,
            muted: false,
            delay: 123,
        };
        output.begin(format.clone(), clock, gain).unwrap();
        assert_eq!(output.player.as_ref().unwrap().static_delay_ms(), 123);
        gain.delay = 0;
        output.gain(gain);
        assert_eq!(
            output.player.as_ref().unwrap().static_delay_ms(),
            0,
            "reset before any buffers must update the actual player"
        );

        gain.delay = 123;
        output.gain(gain);
        // The unsynchronized and stale-clock branches both clear the budget,
        // without clearing the player's configured delay.
        output.write(AudioBuffer {
            timestamp: 0,
            samples: vec![0; 1920].into(),
            format,
        });
        assert_eq!(output.queued.delay, 0);
        assert_eq!(output.player.as_ref().unwrap().static_delay_ms(), 123);
        gain.delay = 0;
        output.gain(gain);
        assert_eq!(
            output.player.as_ref().unwrap().static_delay_ms(),
            0,
            "budget reset must not suppress a player delay update"
        );
        output.clear();
    }
}
