//! Album art for the item that is playing.
//!
//! Music Assistant serves artwork from `/imageproxy/<id>?size=&fmt=`, which
//! needs no credentials and resizes server-side, so the client asks for a small
//! image rather than fetching a full-size cover to throw most of it away. The
//! allowed sizes are fixed by the server (`controllers/metadata/constants.py`:
//! 0, 80, 160, 256, 512, 1024) and `fmt=jpg` is requested so only one decoder
//! is needed.
//!
//! Drawing uses half-block characters: the upper half takes the foreground
//! colour and the lower half the background, so one cell carries two pixels.
//! That is coarse — a panel of `n` columns is `n` pixels wide — but it composes
//! with the rest of the interface as ordinary styled cells. Terminal graphics
//! protocols are sharper, but they write bytes the cell renderer does not know
//! about and then have to fight it for the region on every redraw.

use anyhow::{anyhow, Result};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// Sizes the image proxy will serve. Anything else is rejected by the server.
const SIZES: [u16; 5] = [80, 160, 256, 512, 1024];

/// A decoded cover, kept at whatever size the server returned.
#[derive(Clone, PartialEq, Eq)]
pub struct Art {
    width: usize,
    height: usize,
    /// Row-major RGB, three bytes per pixel.
    pixels: Vec<u8>,
}

impl std::fmt::Debug for Art {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Art({}x{})", self.width, self.height)
    }
}

impl Art {
    /// Decode a JPEG cover. Anything that is not one is simply not shown.
    pub fn decode(jpeg: &[u8]) -> Result<Self> {
        let mut decoder = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(jpeg));
        decoder
            .decode_headers()
            .map_err(|_| anyhow!("Unreadable album art"))?;
        let (width, height) = decoder
            .dimensions()
            .ok_or_else(|| anyhow!("Album art has no dimensions"))?;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(anyhow!("Album art is not a usable size"));
        }
        let pixels = decoder
            .decode()
            .map_err(|_| anyhow!("Unreadable album art"))?;
        let pixels = match pixels.len() / (width * height) {
            3 => pixels,
            // A greyscale cover decodes to one channel; widen it rather than
            // refusing to show it.
            1 => pixels.iter().flat_map(|v| [*v, *v, *v]).collect(),
            _ => return Err(anyhow!("Unsupported album art format")),
        };
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// The pixel covering a point in the source, by area average, so shrinking
    /// a cover does not reduce it to whichever pixels happened to be sampled.
    fn block(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Color {
        let (x1, y1) = (
            x1.max(x0 + 1).min(self.width),
            y1.max(y0 + 1).min(self.height),
        );
        let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
        for y in y0..y1 {
            for x in x0..x1 {
                let offset = (y * self.width + x) * 3;
                if let Some(pixel) = self.pixels.get(offset..offset + 3) {
                    r += pixel[0] as u32;
                    g += pixel[1] as u32;
                    b += pixel[2] as u32;
                    n += 1;
                }
            }
        }
        if n == 0 {
            return Color::Reset;
        }
        Color::Rgb((r / n) as u8, (g / n) as u8, (b / n) as u8)
    }

    /// Render into `columns` by `rows` cells. Each cell is one pixel wide and
    /// two tall, so a square cover wants twice as many columns as rows.
    pub fn half_blocks(&self, columns: u16, rows: u16) -> Vec<Line<'static>> {
        let (columns, rows) = (columns as usize, rows as usize);
        let mut lines = Vec::with_capacity(rows);
        if columns == 0 || rows == 0 {
            return lines;
        }
        let span = |n: usize, of: usize, total: usize| {
            (
                n * total / of,
                ((n + 1) * total / of).max(n * total / of + 1),
            )
        };
        for row in 0..rows {
            let mut cells = Vec::with_capacity(columns);
            for column in 0..columns {
                let (x0, x1) = span(column, columns, self.width);
                // Two pixel rows per cell: the upper half is drawn, the lower
                // half is the cell's own background showing through.
                let (top, bottom) = (row * 2, row * 2 + 1);
                let (y0, y1) = span(top, rows * 2, self.height);
                let (y2, y3) = span(bottom, rows * 2, self.height);
                cells.push(Span::styled(
                    "▀",
                    Style::default()
                        .fg(self.block(x0, y0, x1, y1))
                        .bg(self.block(x0, y2, x1, y3)),
                ));
            }
            lines.push(Line::from(cells));
        }
        lines
    }
}

/// Colours per channel in the fixed palette sixel output quantizes to. A 6x6x6
/// cube is 216 colours, enough for a cover and small enough to emit inline
/// without the cost and complexity of building a palette per image.
const CUBE: usize = 6;

impl Art {
    /// Encode as sixel at a pixel size, for a terminal that draws bitmaps.
    ///
    /// Sixel packs six vertical pixels into one character, one band at a time,
    /// emitting each colour's pixels across the whole band before moving on.
    /// Colours are quantized to a fixed 6x6x6 cube, so no palette has to be
    /// derived from the image or agreed with the terminal.
    pub fn sixel(&self, width: u16, height: u16) -> String {
        let (width, height) = (width.max(1) as usize, height.max(1) as usize);
        let mut out = String::with_capacity(width * height / 2);
        // P1=7 is a 1:1 pixel aspect: the original spec reads 0 as 2:1, which
        // stretches the image on a terminal that honours it over the raster
        // attributes. P2=1 leaves unset pixels alone rather than painting them
        // as background. The raster attributes then give the size to reserve.
        out.push_str(&format!("\x1bP7;1;0q\"1;1;{width};{height}"));
        for index in 0..CUBE * CUBE * CUBE {
            let (r, g, b) = (index / (CUBE * CUBE), (index / CUBE) % CUBE, index % CUBE);
            // Sixel colour components are percentages, not bytes.
            let percent = |v: usize| v * 100 / (CUBE - 1);
            out.push_str(&format!(
                "#{index};2;{};{};{}",
                percent(r),
                percent(g),
                percent(b)
            ));
        }
        // One index per pixel, resampled once rather than per band.
        let mut cells = vec![0u16; width * height];
        let span = |n: usize, of: usize, total: usize| {
            (
                n * total / of,
                ((n + 1) * total / of).max(n * total / of + 1),
            )
        };
        for y in 0..height {
            let (y0, y1) = span(y, height, self.height);
            for x in 0..width {
                let (x0, x1) = span(x, width, self.width);
                let Color::Rgb(r, g, b) = self.block(x0, y0, x1, y1) else {
                    continue;
                };
                let step = |v: u8| (v as usize * (CUBE - 1) + 127) / 255;
                cells[y * width + x] = (step(r) * CUBE * CUBE + step(g) * CUBE + step(b)) as u16;
            }
        }
        for band in 0..height.div_ceil(6) {
            let rows = (band * 6..((band + 1) * 6).min(height)).collect::<Vec<_>>();
            let mut used: Vec<u16> = rows
                .iter()
                .flat_map(|y| cells[y * width..(y + 1) * width].iter().copied())
                .collect();
            used.sort_unstable();
            used.dedup();
            for colour in used {
                out.push_str(&format!("#{colour}"));
                // Run-length encode the band for this colour.
                let (mut run, mut previous) = (0usize, u8::MAX);
                for x in 0..width {
                    let mut bits = 0u8;
                    for (offset, y) in rows.iter().enumerate() {
                        if cells[y * width + x] == colour {
                            bits |= 1 << offset;
                        }
                    }
                    let symbol = b'?' + bits;
                    if symbol == previous {
                        run += 1;
                        continue;
                    }
                    emit(&mut out, previous, run);
                    (previous, run) = (symbol, 1);
                }
                emit(&mut out, previous, run);
                out.push('$');
            }
            out.push('-');
        }
        out.push_str("\x1b\\");
        out
    }
}

/// A run of one sixel character, using the repeat introducer when it pays.
fn emit(out: &mut String, symbol: u8, run: usize) {
    if run == 0 || symbol == u8::MAX {
        return;
    }
    let symbol = symbol as char;
    if run > 3 {
        out.push_str(&format!("!{run}{symbol}"));
    } else {
        for _ in 0..run {
            out.push(symbol);
        }
    }
}

/// A test pattern, for checking what a terminal actually does with sixel
/// without needing a server or a cover: four flat quadrants with a diagonal
/// through them. Wrong colours, a stretched square or banding are all visible
/// at a glance.
pub fn test_pattern(size: usize) -> Art {
    let mut pixels = Vec::with_capacity(size * size * 3);
    for y in 0..size {
        for x in 0..size {
            let (left, top) = (x < size / 2, y < size / 2);
            let rgb = if x.abs_diff(y) < size / 16 {
                [255, 255, 255]
            } else {
                match (left, top) {
                    (true, true) => [220, 40, 40],
                    (false, true) => [40, 180, 60],
                    (true, false) => [50, 90, 220],
                    (false, false) => [230, 190, 40],
                }
            };
            pixels.extend_from_slice(&rgb);
        }
    }
    Art {
        width: size,
        height: size,
        pixels,
    }
}

/// The size of one character cell in pixels, which sixel needs and half blocks
/// do not. A terminal that does not report it cannot be drawn into this way.
pub fn cell_pixels() -> Option<(u16, u16)> {
    let size = crossterm::terminal::window_size().ok()?;
    let (width, height) = (
        size.width.checked_div(size.columns)?,
        size.height.checked_div(size.rows)?,
    );
    (width > 0 && height > 0).then_some((width, height))
}

/// Whether to draw covers as sixel. `Auto` is a guess from the terminal's own
/// name and whether it reports a pixel size — sixel support cannot be read off
/// either, and asking the terminal directly means a handshake in the middle of
/// the input stream. `sixel` and `blocks` in the configuration settle it
/// outright for anyone this guesses wrong about.
pub fn use_sixel(setting: crate::config::AlbumArt) -> bool {
    use crate::config::AlbumArt;
    match setting {
        AlbumArt::Sixel => true,
        AlbumArt::Off | AlbumArt::Blocks => false,
        AlbumArt::Auto => {
            if cell_pixels().is_none() {
                return false;
            }
            let term = std::env::var("TERM").unwrap_or_default();
            let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
            term.starts_with("foot")
                || term.contains("mlterm")
                || term.contains("contour")
                || program.eq_ignore_ascii_case("WezTerm")
        }
    }
}

/// The proxy URL for a cover, asking for the smallest served size that still
/// covers the cells it will be drawn into.
pub fn url(server: &str, proxy_id: &str, columns: u16) -> Result<String> {
    if proxy_id.is_empty() || !proxy_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(anyhow!("Invalid artwork id"));
    }
    let mut url = url::Url::parse(server).map_err(|_| anyhow!("Invalid server URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(anyhow!("Invalid server URL"));
    }
    let size = SIZES
        .iter()
        .find(|size| **size >= columns)
        .copied()
        .unwrap_or(1024);
    url.set_path(&format!(
        "{}/imageproxy/{proxy_id}",
        url.path().trim_end_matches('/')
    ));
    url.set_query(Some(&format!("size={size}&fmt=jpg")));
    Ok(url.into())
}

/// The cover for a queue item, wherever the server put it. A queue item may
/// carry the image on its media item's metadata or directly as a mapping.
pub fn proxy_id(item: &serde_json::Value) -> Option<String> {
    let candidates = [
        &item["media_item"]["metadata"]["images"][0],
        &item["media_item"]["image"],
        &item["metadata"]["images"][0],
        &item["image"],
    ];
    candidates
        .into_iter()
        .filter_map(|image| image["proxy_id"].as_str())
        .find(|id| !id.is_empty())
        .map(str::to_owned)
}
