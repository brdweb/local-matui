//! Spectrum analysis of Matui's own local audio output.
//!
//! The only source is decoded PCM that this process is about to hand to CPAL,
//! tagged with the local instant the synchronized player is scheduled to emit
//! it. A remote speaker's audio never passes through this machine, so there is
//! nothing to analyze then and nothing is invented: the view says why it is
//! empty instead of animating. The scheduled emission instant is not a
//! measurement of device buffering or acoustic latency, so alignment is to what
//! Matui sends, not to what a speaker reproduces.
//!
//! Mute is honoured because muted output is silent. The per-player volume curve
//! is applied downstream by Sendspin's ramped `GainControl`; it is not
//! reproduced here, so bar height reflects the decoded stream rather than a
//! measured output level.
//!
//! `push` runs on the audio worker thread — the thread that already decodes and
//! allocates, never the CPAL callback — and analysis happens there so the ring
//! stays bounded regardless of how far ahead the server streams. Frames are
//! small fixed arrays; the UI thread copies one out under a short lock and
//! renders afterwards.

use crate::theme::Palette;
use parking_lot::Mutex;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{
    collections::VecDeque,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

/// FFT length: 42.7 ms and 23.4 Hz of resolution at 48 kHz.
pub const WINDOW: usize = 2048;
/// Analysis bands produced per frame, aggregated further for the display width.
pub const BANDS: usize = 64;
/// Analysis range. A 2048-point window has no usable resolution below this,
/// and 16 kHz is the top of the range most listeners and codecs retain.
const MIN_HZ: f32 = 40.0;
const MAX_HZ: f32 = 16_000.0;
/// Displayed dynamic range: this many dBFS below full scale is an empty bar.
const FLOOR_DB: f32 = -66.0;
/// Bands above 200 Hz are lifted by this much per octave so ordinary music
/// fills the display rather than hugging the bottom. Presentation only.
const TILT_DB_PER_OCTAVE: f32 = 3.0;
const MAX_TILT_DB: f32 = 20.0;
/// One analysis frame per this much audio, at any sample rate.
const FRAME_US: u64 = 21_000;
/// Frames retained ahead of playback: about 34 seconds, which covers the
/// 35-second scheduling horizon the decoded queue allows. Bounded memory
/// matters more than analyzing audio queued beyond it.
const MAX_FRAMES: usize = 1_600;
/// A frame older than this is not being played: the stream stopped or paused.
const STALE: Duration = Duration::from_millis(300);
/// Chunks whose scheduled instants differ by more than this are not contiguous;
/// the sample history restarts rather than splicing unrelated audio.
const DISCONTINUITY: Duration = Duration::from_millis(60);

/// Analysis bands for one instant, as fractions of the displayed range.
pub type Spectrum = [u8; BANDS];

struct Frame {
    /// Local instant this window is scheduled to be emitted.
    deadline: Instant,
    bands: Spectrum,
}

struct State {
    /// Rolling mono history, newest last, for forming overlapping windows.
    history: Vec<f32>,
    /// Mono samples written since the last analyzed window.
    pending: usize,
    rate: u32,
    /// Instant at which the next sample pushed is scheduled to be emitted.
    next: Option<Instant>,
    frames: VecDeque<Frame>,
    muted: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            history: vec![0.0; WINDOW],
            pending: 0,
            rate: 0,
            next: None,
            frames: VecDeque::new(),
            muted: false,
        }
    }
}

impl State {
    fn restart(&mut self, rate: u32) {
        self.history.clear();
        self.history.resize(WINDOW, 0.0);
        self.pending = 0;
        self.rate = rate;
        self.frames.clear();
    }

    /// Keep only the samples a following window can still overlap.
    fn trim(&mut self) {
        let excess = self.history.len().saturating_sub(WINDOW * 2);
        if excess > 0 {
            self.history.drain(..excess);
        }
    }
}

/// Shared handle: the audio worker pushes, the interface captures.
#[derive(Clone, Default)]
pub struct Analyzer {
    state: Arc<Mutex<State>>,
}

impl Analyzer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record decoded interleaved samples scheduled for emission at `emitted`.
    pub fn push(&self, samples: &[i32], channels: u8, rate: u32, emitted: Instant) {
        let channels = channels as usize;
        if channels == 0 || rate == 0 || samples.len() < channels {
            return;
        }
        let frames = samples.len() / channels;
        let mut state = self.state.lock();
        let contiguous = state.rate == rate
            && state.next.is_some_and(|next| {
                next.checked_duration_since(emitted)
                    .unwrap_or_default()
                    .max(emitted.checked_duration_since(next).unwrap_or_default())
                    < DISCONTINUITY
            });
        if !contiguous {
            state.restart(rate);
        }
        // Analysis is spaced by audio duration, so a frame covers the same
        // amount of time at any sample rate.
        let hop = ((rate as u64 * FRAME_US / 1_000_000) as usize).max(256);
        for (index, frame) in samples.chunks_exact(channels).enumerate() {
            let sum: i64 = frame.iter().map(|s| i64::from(*s)).sum();
            state
                .history
                .push(sum as f32 / channels as f32 / i32::MAX as f32);
            state.pending += 1;
            if state.pending < hop {
                continue;
            }
            state.pending = 0;
            if state.frames.len() >= MAX_FRAMES {
                // Dropping the newest keeps the frames nearest playback intact.
                continue;
            }
            // Emission instant of the newest sample in this window.
            let elapsed = Duration::from_nanos((index as u64 + 1) * 1_000_000_000 / rate as u64);
            let Some(deadline) = emitted.checked_add(elapsed) else {
                continue;
            };
            let start = state.history.len() - WINDOW;
            let bands = analyze(&state.history[start..], rate);
            state.frames.push_back(Frame { deadline, bands });
            state.trim();
        }
        let elapsed = Duration::from_nanos(frames as u64 * 1_000_000_000 / rate as u64);
        state.next = emitted.checked_add(elapsed);
        state.trim();
    }

    /// Record the output mute state; muted output is silent, so it is shown so.
    pub fn set_muted(&self, muted: bool) {
        self.state.lock().muted = muted;
    }

    /// Discard analysis for a stream that has ended or been replaced.
    pub fn clear(&self) {
        let mut state = self.state.lock();
        state.restart(0);
        state.next = None;
    }

    /// The bands scheduled to be emitting at `now`, or a short reason there are
    /// none. Frames already played are dropped here rather than on the audio
    /// thread.
    pub fn capture(&self, now: Instant) -> Result<Spectrum, &'static str> {
        let mut state = self.state.lock();
        while state.frames.len() > 1 && state.frames[1].deadline <= now {
            state.frames.pop_front();
        }
        if state.muted {
            return Err("output muted");
        }
        let Some(frame) = state.frames.front() else {
            return Err("no local audio");
        };
        if frame.deadline > now {
            return Err("buffering");
        }
        if now.duration_since(frame.deadline) > STALE {
            return Err("no local audio");
        }
        Ok(frame.bands)
    }

    /// Sample rate of the stream being analyzed, for frequency labels.
    pub fn rate(&self) -> u32 {
        self.state.lock().rate
    }

    /// Frames retained ahead of playback, bounded by `MAX_FRAMES`.
    pub fn buffered(&self) -> usize {
        self.state.lock().frames.len()
    }
}

/// The audio output writes what it is about to play through this.
impl crate::audio::SampleSink for Analyzer {
    fn push(&self, samples: &[i32], channels: u8, rate: u32, emitted: Instant) {
        Analyzer::push(self, samples, channels, rate, emitted);
    }
    fn set_muted(&self, muted: bool) {
        Analyzer::set_muted(self, muted);
    }
    fn clear(&self) {
        Analyzer::clear(self);
    }
}

/// Hann window coefficients; the taper keeps a tone in its own band.
fn hann() -> &'static [f32; WINDOW] {
    static HANN: OnceLock<[f32; WINDOW]> = OnceLock::new();
    HANN.get_or_init(|| {
        let mut window = [0.0; WINDOW];
        for (index, value) in window.iter_mut().enumerate() {
            let phase = std::f64::consts::TAU * index as f64 / WINDOW as f64;
            *value = (0.5 - 0.5 * phase.cos()) as f32;
        }
        window
    })
}

/// Twiddle factors for the forward transform, indexed by half-window position.
fn twiddles() -> &'static [(f32, f32); WINDOW / 2] {
    static TWIDDLES: OnceLock<[(f32, f32); WINDOW / 2]> = OnceLock::new();
    TWIDDLES.get_or_init(|| {
        let mut table = [(0.0, 0.0); WINDOW / 2];
        for (index, value) in table.iter_mut().enumerate() {
            let angle = -std::f64::consts::TAU * index as f64 / WINDOW as f64;
            *value = (angle.cos() as f32, angle.sin() as f32);
        }
        table
    })
}

/// In-place iterative radix-2 transform over a fixed power-of-two length.
fn transform(re: &mut [f32; WINDOW], im: &mut [f32; WINDOW]) {
    let table = twiddles();
    let mut target = 0usize;
    for source in 1..WINDOW {
        let mut bit = WINDOW >> 1;
        while target & bit != 0 {
            target ^= bit;
            bit >>= 1;
        }
        target |= bit;
        if source < target {
            re.swap(source, target);
            im.swap(source, target);
        }
    }
    let mut span = 2;
    while span <= WINDOW {
        let half = span / 2;
        let step = WINDOW / span;
        let mut base = 0;
        while base < WINDOW {
            for offset in 0..half {
                let (cos, sin) = table[offset * step];
                let (top, bottom) = (base + offset, base + offset + half);
                let (real, imaginary) = (
                    re[bottom] * cos - im[bottom] * sin,
                    re[bottom] * sin + im[bottom] * cos,
                );
                re[bottom] = re[top] - real;
                im[bottom] = im[top] - imaginary;
                re[top] += real;
                im[top] += imaginary;
            }
            base += span;
        }
        span <<= 1;
    }
}

/// Magnitude spectrum of one window, as fractions of the displayed range.
/// Fewer than `WINDOW` samples are zero-padded, which only lowers the result.
pub fn analyze(samples: &[f32], rate: u32) -> Spectrum {
    let mut bands = [0u8; BANDS];
    if rate == 0 || samples.is_empty() {
        return bands;
    }
    let window = hann();
    let mut re = [0.0f32; WINDOW];
    let mut im = [0.0f32; WINDOW];
    let start = samples.len().saturating_sub(WINDOW);
    for (index, sample) in samples[start..].iter().enumerate() {
        re[index] = sample * window[index];
    }
    transform(&mut re, &mut im);
    // A full-scale sine peaks at WINDOW/4 through a Hann window.
    let scale = 4.0 / WINDOW as f32;
    let top = (MAX_HZ.min(rate as f32 / 2.0)).max(MIN_HZ * 2.0);
    let ratio = (top / MIN_HZ).powf(1.0 / BANDS as f32);
    let per_bin = rate as f32 / WINDOW as f32;
    let mut edge = MIN_HZ;
    for band in bands.iter_mut() {
        let next = edge * ratio;
        let first = ((edge / per_bin).round() as usize).max(1);
        let last = ((next / per_bin).round() as usize)
            .max(first + 1)
            .min(WINDOW / 2);
        let mut peak = 0.0f32;
        for bin in first..last {
            let magnitude = (re[bin] * re[bin] + im[bin] * im[bin]).sqrt() * scale;
            peak = peak.max(magnitude);
        }
        let center = (edge * next).sqrt();
        let tilt = (TILT_DB_PER_OCTAVE * (center / 200.0).max(1.0).log2()).min(MAX_TILT_DB);
        let decibels = 20.0 * peak.max(1e-9).log10() + tilt;
        let level = ((decibels - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
        *band = (level * 255.0).round() as u8;
        edge = next;
    }
    bands
}

/// How much of the interface the visualizer occupies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    /// Replaces the browser/queue pane, leaving the rest of the interface.
    Panel,
    /// Takes over the whole terminal.
    Full,
}

impl Mode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Panel,
            Self::Panel => Self::Full,
            Self::Full => Self::Off,
        }
    }
}

/// Display state: smoothing and peak fall are presentation, kept out of the
/// analysis so captured bands stay a plain measurement of the decoded window.
#[derive(Default)]
pub struct Meter {
    pub mode: Mode,
    levels: Vec<f32>,
    peaks: Vec<f32>,
    updated: Option<Instant>,
}

/// Full-scale fall per second once a band stops being driven.
const FALL_PER_SECOND: f32 = 1.9;
/// Peak marker fall per second.
const PEAK_FALL_PER_SECOND: f32 = 0.55;

impl Meter {
    /// Advance the display towards `bands`, or towards silence without them.
    pub fn update(&mut self, bands: Option<Spectrum>, bars: usize, now: Instant) {
        let elapsed = self
            .updated
            .map(|last| now.saturating_duration_since(last).as_secs_f32())
            .unwrap_or_default()
            .clamp(0.0, 0.25);
        self.updated = Some(now);
        if self.levels.len() != bars {
            self.levels = vec![0.0; bars];
            self.peaks = vec![0.0; bars];
        }
        for index in 0..bars {
            // Each bar covers a contiguous run of analysis bands.
            let target = match bands {
                Some(bands) if bars > 0 => {
                    let first = index * BANDS / bars;
                    let last = (((index + 1) * BANDS / bars).max(first + 1)).min(BANDS);
                    bands[first..last]
                        .iter()
                        .fold(0u8, |peak, band| peak.max(*band)) as f32
                        / 255.0
                }
                _ => 0.0,
            };
            let level = &mut self.levels[index];
            // Immediate attack keeps transients visible; the fall is smoothed.
            *level = if target > *level {
                target
            } else {
                (*level - FALL_PER_SECOND * elapsed).max(target.max(0.0))
            };
            let peak = &mut self.peaks[index];
            *peak = if *level >= *peak {
                *level
            } else {
                (*peak - PEAK_FALL_PER_SECOND * elapsed).max(*level)
            };
        }
    }

    pub fn levels(&self) -> &[f32] {
        &self.levels
    }

    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }
}

const BLOCKS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
/// Eighths of a cell, the vertical resolution one row provides.
const STEPS: f32 = 8.0;

/// Bar width and gap for an area, chosen so bars stay chunky when there is room.
pub fn columns(width: u16) -> (usize, usize, usize) {
    let (bar, gap) = if width >= 60 { (2, 1) } else { (1, 0) };
    let bars = ((width as usize + gap) / (bar + gap)).max(1);
    (bars, bar, gap)
}

/// Draw the bars over `area`. `reason` replaces them with a flat baseline and
/// an explanation when there is no local audio to show.
pub fn render(
    frame: &mut ratatui::Frame,
    area: Rect,
    palette: Palette,
    meter: &Meter,
    reason: Option<&str>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (bars, bar, gap) = columns(area.width);
    let height = area.height as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(height);
    if let Some(reason) = reason {
        // A flat baseline with the reason: never motion that means nothing.
        for row in 0..height {
            if row + 1 == height {
                let dots = "·".repeat(area.width as usize);
                lines.push(Line::styled(dots, Style::default().fg(palette.secondary)));
            } else if row + 2 == height && area.width as usize > reason.len() + 2 {
                lines.push(Line::styled(
                    format!(" {reason}"),
                    Style::default().fg(palette.secondary),
                ));
            } else {
                lines.push(Line::raw(""));
            }
        }
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }
    let levels = meter.levels();
    let peaks = meter.peaks();
    for row in 0..height {
        // Rows are drawn top down; a bar grows from the bottom.
        let from_bottom = height - 1 - row;
        let mut spans: Vec<Span> = Vec::with_capacity(bars * 2);
        for index in 0..bars {
            if index > 0 && gap > 0 {
                spans.push(Span::raw(" ".repeat(gap)));
            }
            let level = levels.get(index).copied().unwrap_or(0.0);
            let peak = peaks.get(index).copied().unwrap_or(0.0);
            let filled = level * height as f32 * STEPS;
            let step = (filled - from_bottom as f32 * STEPS).clamp(0.0, STEPS) as usize;
            let peak_row = (peak * height as f32).min(height as f32 - 0.001) as usize;
            let (symbol, style) = if step > 0 {
                (BLOCKS[step], Style::default().fg(palette.accent))
            } else if peak > 0.0 && peak_row == from_bottom {
                // The marker floats above the bar it belongs to.
                ("▄", Style::default().fg(palette.secondary))
            } else {
                (" ", Style::default())
            };
            spans.push(Span::styled(symbol.repeat(bar), style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Frequency ruler aligned to `render`'s bars, for the full-screen view.
/// Labels are written at the column of the bar covering each decade.
pub fn scale(width: u16, rate: u32) -> String {
    let (bars, bar, gap) = columns(width);
    let mut ruler = vec![' '; width as usize];
    let top = (MAX_HZ.min(rate.max(1) as f32 / 2.0)).max(MIN_HZ * 2.0);
    let ratio = (top / MIN_HZ).powf(1.0 / BANDS as f32);
    for mark in [100.0f32, 1_000.0, 10_000.0] {
        if mark > top {
            continue;
        }
        let band = (mark / MIN_HZ).log(ratio).floor().max(0.0) as usize;
        if band >= BANDS {
            continue;
        }
        // Bars aggregate bands the same way `Meter::update` does.
        let column = (band * bars / BANDS) * (bar + gap);
        let text = if mark >= 1_000.0 {
            format!("{:.0}k", mark / 1_000.0)
        } else {
            format!("{mark:.0}")
        };
        for (offset, symbol) in text.chars().enumerate() {
            if let Some(slot) = ruler.get_mut(column + offset) {
                *slot = symbol;
            }
        }
    }
    ruler.into_iter().collect()
}
