//! Album art. The only image used is generated here; no cover or personal
//! media is read, fetched or committed.
use local_matui::artwork::{proxy_id, url, Art};
use ratatui::style::Color;
use serde_json::json;

/// A real 8x8 solid-red baseline JPEG, generated once with
/// `magick -size 8x8 xc:'#ff0000' -quality 90 red8.jpg` and inlined so the test
/// needs neither ImageMagick nor a binary fixture in the tree. It is a
/// generated colour field, not anyone's cover art.
fn red_jpeg() -> Vec<u8> {
    const BYTES: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x03, 0x02, 0x02, 0x03, 0x02,
        0x02, 0x03, 0x03, 0x03, 0x03, 0x04, 0x03, 0x03, 0x04, 0x05, 0x08, 0x05, 0x05, 0x04, 0x04,
        0x05, 0x0A, 0x07, 0x07, 0x06, 0x08, 0x0C, 0x0A, 0x0C, 0x0C, 0x0B, 0x0A, 0x0B, 0x0B, 0x0D,
        0x0E, 0x12, 0x10, 0x0D, 0x0E, 0x11, 0x0E, 0x0B, 0x0B, 0x10, 0x16, 0x10, 0x11, 0x13, 0x14,
        0x15, 0x15, 0x15, 0x0C, 0x0F, 0x17, 0x18, 0x16, 0x14, 0x18, 0x12, 0x14, 0x15, 0x14, 0xFF,
        0xDB, 0x00, 0x43, 0x01, 0x03, 0x04, 0x04, 0x05, 0x04, 0x05, 0x09, 0x05, 0x05, 0x09, 0x14,
        0x0D, 0x0B, 0x0D, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x08,
        0x00, 0x08, 0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xFF, 0xC4, 0x00,
        0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x08, 0xFF, 0xC4, 0x00, 0x14, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xC4, 0x00, 0x15,
        0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x07, 0x09, 0xFF, 0xC4, 0x00, 0x14, 0x11, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xDA, 0x00, 0x0C,
        0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3F, 0x00, 0x3A, 0x03, 0x15, 0x4D, 0xFF,
        0xD9,
    ];
    BYTES.to_vec()
}

#[test]
fn a_cover_decodes_and_fills_its_cells_with_the_image_colour() {
    let art = Art::decode(&red_jpeg()).expect("a baseline JPEG must decode");
    // Two pixels per cell, so eight rows of cells cover sixteen pixel rows.
    let lines = art.half_blocks(8, 4);
    assert_eq!(lines.len(), 4, "one line per cell row");
    for line in &lines {
        assert_eq!(line.spans.len(), 8, "one span per column");
        for span in &line.spans {
            assert_eq!(span.content, "▀", "a cell is one half block");
            let (Some(Color::Rgb(r, g, b)), Some(Color::Rgb(br, bg, bb))) =
                (span.style.fg, span.style.bg)
            else {
                panic!("both halves of a cell must carry a colour");
            };
            // The source is solid red; JPEG is lossy, so this is approximate.
            for (red, green, blue) in [(r, g, b), (br, bg, bb)] {
                assert!(
                    red > 150 && green < 110 && blue < 110,
                    "a red cover must draw red, got {red},{green},{blue}"
                );
            }
        }
    }
    // Degenerate sizes must not panic or produce rows that cannot be drawn.
    assert!(art.half_blocks(0, 0).is_empty());
    assert_eq!(art.half_blocks(1, 1).len(), 1);
    assert_eq!(art.half_blocks(200, 60).len(), 60);
}

#[test]
fn anything_that_is_not_an_image_is_simply_not_shown() {
    assert!(Art::decode(b"").is_err());
    assert!(Art::decode(b"<html>not an image</html>").is_err());
    // A truncated cover must fail rather than draw whatever decoded so far.
    let mut truncated = red_jpeg();
    truncated.truncate(truncated.len() / 2);
    assert!(Art::decode(&truncated).is_err());
}

#[test]
fn the_proxy_url_asks_for_a_served_size_and_refuses_an_unsafe_id() {
    // Only the sizes the server serves: 80 is the smallest, so a small panel
    // still asks for 80 rather than an arbitrary number.
    assert_eq!(
        url("http://host:8095", "abc123", 24).unwrap(),
        "http://host:8095/imageproxy/abc123?size=80&fmt=jpg"
    );
    assert_eq!(
        url("http://host:8095", "abc123", 200).unwrap(),
        "http://host:8095/imageproxy/abc123?size=256&fmt=jpg"
    );
    // A reverse-proxy prefix survives, as it does for the API and the socket.
    assert_eq!(
        url("https://host/music/", "abc", 24).unwrap(),
        "https://host/music/imageproxy/abc?size=80&fmt=jpg"
    );
    // The id goes into a URL path, so it is never taken on trust.
    for bad in ["", "../../etc/passwd", "a/b", "a?b", "a b", "a#b"] {
        assert!(
            url("http://host", bad, 24).is_err(),
            "{bad} must be refused"
        );
    }
}

#[test]
fn the_cover_is_found_wherever_the_server_puts_it() {
    let nested = json!({"media_item": {"metadata": {"images": [{"proxy_id": "deadbeef"}]}}});
    assert_eq!(proxy_id(&nested).as_deref(), Some("deadbeef"));

    let mapping = json!({"media_item": {"image": {"proxy_id": "cafe"}}});
    assert_eq!(proxy_id(&mapping).as_deref(), Some("cafe"));

    let direct = json!({"image": {"proxy_id": "f00d"}});
    assert_eq!(proxy_id(&direct).as_deref(), Some("f00d"));

    // An item with no artwork is not an error, it just has none.
    assert_eq!(proxy_id(&json!({})), None);
    assert_eq!(proxy_id(&json!({"image": {"proxy_id": ""}})), None);
    assert_eq!(proxy_id(&json!({"media_item": {"metadata": {}}})), None);
}

/// The cover sits in the player, beside what is playing rather than instead of
/// it, and the interface is unchanged when there is no cover to show.
#[test]
fn the_player_shows_the_cover_beside_the_track_it_belongs_to() {
    use local_matui::ui;
    let render = |art: Option<Art>| {
        let mut app = ui::App {
            artwork: art,
            connected: true,
            selected_id: Some("kitchen".into()),
            players: vec![ui::PlayerView {
                id: "kitchen".into(),
                name: "Kitchen".into(),
                available: true,
                state: "playing".into(),
                ..Default::default()
            }],
            title: "Something To Play".into(),
            artist: "An Artist".into(),
            ..Default::default()
        };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(110, 30)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        let coloured = buffer
            .content
            .iter()
            .filter(|cell| matches!(cell.fg, Color::Rgb(r, g, b) if r > 150 && g < 110 && b < 110))
            .count();
        (text, coloured)
    };

    let (with_art, coloured) = render(Some(Art::decode(&red_jpeg()).unwrap()));
    assert!(coloured > 20, "the cover is drawn in its own colours");
    assert!(
        with_art.contains("Something To Play") && with_art.contains("An Artist"),
        "the track keeps its place beside the cover"
    );
    assert!(with_art.contains("PLAYING"), "so does the transport row");

    let (without, coloured) = render(None);
    assert_eq!(coloured, 0, "no cover, no colour");
    assert!(without.contains("Something To Play"));
}
