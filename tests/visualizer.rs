//! Spectrum analysis and visualizer presentation. Signals here are generated
//! in the test, not captured from any device or personal media.
use matui::visualizer::{self, Analyzer, Meter, Mode, Spectrum, BANDS, WINDOW};
use std::time::{Duration, Instant};

const RATE: u32 = 48_000;

/// Interleaved stereo full-scale sine, as the decoders produce it.
fn tone(hertz: f32, frames: usize, amplitude: f32) -> Vec<i32> {
    (0..frames)
        .flat_map(|frame| {
            let phase = std::f32::consts::TAU * hertz * frame as f32 / RATE as f32;
            let sample = (phase.sin() * amplitude * i32::MAX as f32) as i32;
            [sample, sample]
        })
        .collect()
}

fn mono(hertz: f32, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|frame| (std::f32::consts::TAU * hertz * frame as f32 / RATE as f32).sin())
        .collect()
}

/// Band a frequency falls in, matching the module's logarithmic layout.
fn band_of(hertz: f32) -> usize {
    let ratio = (16_000f32 / 40.0).powf(1.0 / BANDS as f32);
    (hertz / 40.0).log(ratio).floor() as usize
}

fn loudest(bands: &Spectrum) -> usize {
    bands
        .iter()
        .enumerate()
        .max_by_key(|(_, level)| **level)
        .map(|(index, _)| index)
        .unwrap()
}

#[test]
fn analysis_puts_a_tone_in_its_own_band_and_leaves_silence_empty() {
    for hertz in [200.0, 1_000.0, 5_000.0] {
        let bands = visualizer::analyze(&mono(hertz, WINDOW), RATE);
        let expected = band_of(hertz);
        let peak = loudest(&bands);
        assert!(
            peak.abs_diff(expected) <= 1,
            "{hertz} Hz landed in band {peak}, expected about {expected}"
        );
        assert!(bands[peak] > 200, "a full-scale tone must fill its band");
        // Bands a decade away from the tone must stay near the floor.
        let far = band_of(hertz / 8.0);
        assert!(
            bands[far] < 64,
            "{hertz} Hz leaked into band {far} at {}",
            bands[far]
        );
    }
    assert_eq!(visualizer::analyze(&vec![0.0; WINDOW], RATE), [0u8; BANDS]);
    assert_eq!(visualizer::analyze(&mono(1_000.0, WINDOW), 0), [0u8; BANDS]);
}

#[test]
fn quieter_audio_produces_shorter_bars() {
    let loud = visualizer::analyze(&mono(1_000.0, WINDOW), RATE);
    let quiet: Vec<f32> = mono(1_000.0, WINDOW).iter().map(|s| s * 0.05).collect();
    let quiet = visualizer::analyze(&quiet, RATE);
    let band = band_of(1_000.0);
    assert!(
        quiet[band] < loud[band],
        "a 26 dB drop must lower the bar: {} vs {}",
        quiet[band],
        loud[band]
    );
    assert!(quiet[band] > 0, "audible audio must still register");
}

#[test]
fn capture_follows_the_instant_each_window_is_scheduled_to_be_emitted() {
    let analyzer = Analyzer::new();
    let now = Instant::now();
    let lead = Duration::from_millis(500);
    // Four windows' worth of audio, scheduled half a second ahead as the
    // player's required lead time implies.
    analyzer.push(&tone(1_000.0, 8_192, 1.0), 2, RATE, now + lead);

    assert_eq!(analyzer.capture(now), Err("buffering"));
    let playing = analyzer
        .capture(now + lead + Duration::from_millis(120))
        .expect("audio scheduled for this instant must be analyzed");
    assert!(
        loudest(&playing).abs_diff(band_of(1_000.0)) <= 1,
        "the captured window must be the tone that was pushed"
    );
    assert_eq!(
        analyzer.capture(now + Duration::from_secs(5)),
        Err("no local audio"),
        "audio whose emission has long passed is not playing"
    );
    assert_eq!(
        Analyzer::new().capture(now),
        Err("no local audio"),
        "an endpoint that has never received audio shows nothing"
    );
}

#[test]
fn muted_output_reports_silence_instead_of_bars() {
    let analyzer = Analyzer::new();
    let now = Instant::now();
    analyzer.push(&tone(1_000.0, 8_192, 1.0), 2, RATE, now);
    analyzer.set_muted(true);
    assert_eq!(
        analyzer.capture(now + Duration::from_millis(100)),
        Err("output muted")
    );
    analyzer.set_muted(false);
    assert!(analyzer.capture(now + Duration::from_millis(100)).is_ok());
    analyzer.clear();
    assert_eq!(
        analyzer.capture(now + Duration::from_millis(100)),
        Err("no local audio"),
        "a cleared stream leaves nothing behind"
    );
}

#[test]
fn frames_stay_bounded_when_the_server_streams_far_ahead() {
    let analyzer = Analyzer::new();
    let start = Instant::now();
    let chunk = tone(440.0, 48_000, 0.5);
    // Sixty contiguous seconds queued ahead of playback, beyond the horizon
    // the decoded queue permits.
    for second in 0..60 {
        analyzer.push(&chunk, 2, RATE, start + Duration::from_secs(second));
    }
    let buffered = analyzer.buffered();
    assert!(
        buffered <= 1_600,
        "retained frames must stay bounded, found {buffered}"
    );
    assert!(
        buffered > 1_000,
        "the retained frames must still cover the scheduling horizon"
    );
    assert!(
        analyzer.capture(start + Duration::from_millis(100)).is_ok(),
        "the frames nearest playback must survive the bound"
    );
}

#[test]
fn a_gap_in_the_stream_restarts_analysis_rather_than_splicing() {
    let analyzer = Analyzer::new();
    let start = Instant::now();
    analyzer.push(&tone(1_000.0, 8_192, 1.0), 2, RATE, start);
    // A new stream a minute later must not be joined to the old samples.
    let later = start + Duration::from_secs(60);
    analyzer.push(&tone(1_000.0, 1_024, 1.0), 2, RATE, later);
    // Only the new stream remains: nothing is scheduled for the old instant,
    // and what is queued is not due yet.
    assert_eq!(
        analyzer.capture(start + Duration::from_millis(100)),
        Err("buffering"),
        "frames from the abandoned stream must be discarded"
    );
    // The first window of the new stream completes one hop in, at 21 ms.
    assert!(
        analyzer.capture(later + Duration::from_millis(30)).is_ok(),
        "the new stream must be analyzed on its own timeline"
    );
    assert_eq!(analyzer.rate(), RATE);
}

#[test]
fn the_meter_rises_at_once_and_falls_back_over_time() {
    let mut meter = Meter::default();
    let start = Instant::now();
    let mut bands = [0u8; BANDS];
    bands[10] = 255;
    meter.update(Some(bands), 8, start);
    let bar = 10 * 8 / BANDS;
    assert!(
        meter.levels()[bar] > 0.9,
        "a loud band must reach its bar immediately"
    );
    assert!(meter.peaks()[bar] > 0.9);

    meter.update(None, 8, start + Duration::from_millis(100));
    let falling = meter.levels()[bar];
    assert!(
        falling > 0.0 && falling < 0.9,
        "the fall is smoothed, not instant: {falling}"
    );
    assert!(
        meter.peaks()[bar] > falling,
        "the peak marker trails above the bar"
    );
    // Decay is per frame and clamped, so a stalled interface cannot make the
    // display jump; several seconds of frames settle it at the baseline.
    for frame in 1..=40 {
        meter.update(None, 8, start + Duration::from_millis(100 * frame));
    }
    assert_eq!(meter.levels()[bar], 0.0, "silence settles at the baseline");
    assert_eq!(meter.peaks()[bar], 0.0);
}

fn drawn(width: u16, height: u16, meter: &Meter, reason: Option<&str>) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            visualizer::render(
                frame,
                frame.area(),
                matui::theme::Palette::default(),
                meter,
                reason,
            )
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn rendering_draws_bars_for_audio_and_an_explanation_without_it() {
    let mut meter = Meter::default();
    let (bars, _, _) = visualizer::columns(100);
    let mut bands = [0u8; BANDS];
    bands[32] = 255;
    meter.update(Some(bands), bars, Instant::now());
    let picture = drawn(100, 12, &meter, None);
    assert!(picture.contains('█'), "audio must draw solid bars");

    let empty = drawn(100, 12, &meter, Some("no local audio · playing on Kitchen"));
    assert!(
        empty.contains("no local audio · playing on Kitchen"),
        "an empty visualizer must say why"
    );
    assert!(
        !empty.contains('█'),
        "no bars may be drawn without local samples"
    );
    // Sizes below the interface minimum must still not panic.
    for (width, height) in [(1, 1), (3, 2), (200, 60), (0, 0)] {
        drawn(width.max(1), height.max(1), &meter, None);
        drawn(width.max(1), height.max(1), &meter, Some("no local audio"));
    }
}

#[test]
fn the_frequency_ruler_lines_up_with_the_bars_it_labels() {
    let ruler = visualizer::scale(100, RATE);
    let (_, bar, gap) = visualizer::columns(100);
    assert!(ruler.len() <= 100);
    for mark in ["100", "1k", "10k"] {
        let column = ruler.find(mark).unwrap_or_else(|| panic!("{mark} missing"));
        assert_eq!(
            column % (bar + gap),
            0,
            "{mark} must start at a bar, not inside one"
        );
    }
    assert!(
        visualizer::scale(100, RATE).find("10k") > visualizer::scale(100, RATE).find("100"),
        "labels must ascend with frequency"
    );
    // A rate whose Nyquist limit excludes a decade simply omits its label.
    assert!(!visualizer::scale(100, 8_000).contains("10k"));
    assert!(visualizer::scale(0, RATE).is_empty());
}

#[test]
fn modes_cycle_through_panel_and_full_screen() {
    assert_eq!(Mode::default(), Mode::Off);
    assert_eq!(Mode::Off.next(), Mode::Panel);
    assert_eq!(Mode::Panel.next(), Mode::Full);
    assert_eq!(Mode::Full.next(), Mode::Off);
}

/// Prints both views with a generated signal:
/// `cargo test --test visualizer -- --ignored --nocapture preview`.
#[test]
#[ignore = "prints a picture for inspection rather than asserting"]
fn preview() {
    let analyzer = Analyzer::new();
    let now = Instant::now();
    // A chord plus some high content, so the bars are not one spike.
    let frames = 8_192;
    let mixed: Vec<i32> = (0..frames)
        .flat_map(|frame| {
            let at =
                |hertz: f32| (std::f32::consts::TAU * hertz * frame as f32 / RATE as f32).sin();
            let sample = (0.45 * at(80.0)
                + 0.35 * at(220.0)
                + 0.30 * at(880.0)
                + 0.18 * at(3_500.0)
                + 0.10 * at(9_000.0))
                * i32::MAX as f32
                * 0.8;
            [sample as i32, sample as i32]
        })
        .collect();
    analyzer.push(&mixed, 2, RATE, now - Duration::from_millis(100));

    let mut app = matui::ui::App {
        spectrum: Some(analyzer),
        connected: true,
        selected_id: Some("local".into()),
        local_endpoint: Some("local".into()),
        players: vec![matui::ui::PlayerView {
            id: "local".into(),
            name: "This computer".into(),
            available: true,
            state: "playing".into(),
            volume: Some(30),
            ..Default::default()
        }],
        title: "Generated test tone".into(),
        artist: "Signal generator".into(),
        elapsed: 72.0,
        duration: 240.0,
        status: "Preview".into(),
        ..Default::default()
    };
    for mode in [Mode::Panel, Mode::Full] {
        app.visualizer.mode = mode;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(110, 30)).unwrap();
        // Two frames: the meter needs one to rise before it is drawn.
        for _ in 0..2 {
            terminal
                .draw(|frame| matui::ui::draw(frame, &mut app))
                .unwrap();
        }
        println!("\n=== {mode:?} ===");
        for row in terminal.backend().buffer().content.chunks(110) {
            println!("{}", row.iter().map(|c| c.symbol()).collect::<String>());
        }
    }
}

/// The audio output writes through the sink trait; the adapter must reach the
/// same analyzer the interface reads.
#[test]
fn the_analyzer_is_the_sink_the_audio_output_writes_to() {
    use matui::audio::SampleSink;
    let analyzer = Analyzer::new();
    let sink: &dyn SampleSink = &analyzer;
    let now = Instant::now();
    sink.push(&tone(1_000.0, 8_192, 1.0), 2, RATE, now);
    let at = now + Duration::from_millis(100);
    assert!(analyzer.capture(at).is_ok());
    sink.set_muted(true);
    assert_eq!(analyzer.capture(at), Err("output muted"));
    sink.set_muted(false);
    sink.clear();
    assert_eq!(analyzer.capture(at), Err("no local audio"));
}
