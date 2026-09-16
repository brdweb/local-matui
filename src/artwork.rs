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
