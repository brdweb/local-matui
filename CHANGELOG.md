# Changelog

## 0.9.0 — 2026-09-16

First release under the name **ma-tui**, and the first that is not a beta. It is
pre-1.0: the scope below is complete and exercised, but the limitations at the
end of the README are real and unchanged.

### The interface learns about changes instead of asking

- Music Assistant's `/ws` event stream replaces polling. State changes arrive
  when they happen rather than up to two seconds later, and the playback
  position comes straight from the server's own clock, costing no request at
  all. Polling is kept as a fallback — two seconds with no stream, thirty with
  one — so a socket that dies quietly cannot leave the interface stale. The
  header says which of the two is in use.
- The position on screen runs between updates instead of freezing and jumping.
- The interface redraws only when something changed, rather than twenty times a
  second regardless, and reading the library no longer delays a transport key.

### Podcasts and audiobooks

- Podcast and audiobook libraries, a podcast's episodes, and the shelves the
  server maintains: **Continue listening** and **Recently added**.
- **Unplayed podcasts**, assembled here because the server has no filter for it.
- Episodes and audiobooks show their resume point or that they are finished, and
  can be marked played or unplayed. Marking needs no speaker: it is a library
  edit. Progress set elsewhere — in Audiobookshelf or the web interface —
  appears without a refresh.
- A row says what its item belongs to: the show for an episode, the artists and
  album for a track, the authors for an audiobook.

### The player leads

- The layout is built around the player: what is playing, where, how far in, and
  what it sounds like. Panes are separated by rules rather than boxes, the queue
  is a table with a state column, and the focused pane's heading is filled so it
  is findable at a glance.
- The spectrum is part of the player rather than a mode, drawn in braille for
  four times the vertical resolution of block characters. `spectrum = "blocks"`
  restores the block ramp for a font without braille coverage.
- Album art, drawn as sixel where the terminal draws it and as colour half
  blocks everywhere else. `album_art` selects between them, or turns it off.

### Renamed

- The project is **ma-tui**, displayed as **MA-TUI**. The former name shared a
  binary with an existing Matrix TUI also called `matui`. Every earlier name is
  still read where one exists — the `local-matui` and `matui` configuration
  directories and keyring entries, and the `LOCAL_MATUI_TOKEN` and `MATUI_TOKEN`
  overrides — so an existing installation keeps working without re-entering
  credentials. Nothing is copied, moved or deleted, and a saved `player_id` and
  `player_name` are untouched, so Music Assistant sees the same speaker.

## 0.1.0-beta.2 — 2026-09-08

- Rename the project to **local-matui** to avoid clashing with other projects:
  the command, crate, Arch package, desktop entry and repository are now
  `local-matui`, the Flatpak application ID is `io.github.brdweb.LocalMatui`,
  settings live in `local-matui/config.toml` and the token override is
  `LOCAL_MATUI_TOKEN`. Existing installations keep working without re-entering
  credentials: a pre-rename `matui/config.toml`, keyring entry and `MATUI_TOKEN`
  are still read while no current equivalent exists. Nothing is copied, moved or
  deleted, and Music Assistant sees a new speaker identity only if you create
  one. This release replaces v0.1.0-beta.1, which was withdrawn: it was
  published under the former name only hours earlier and is superseded here.

- Rework the controls and layout: the queue stays beside the music browser, the
  header carries transport state, volume, mute and shuffle/repeat, and the hint
  lines follow the focused pane. **p** is now play/pause with tracks on
  **<**/**>**, **z**/**l** toggle shuffle and cycle repeat, **s** stops, and Esc
  cancels or steps back instead of switching panes.
- Volume keys use the server's `volume_up`/`volume_down` commands instead of
  reading the level first, and seeking resolves the target from the displayed
  position, so neither issues a request per keystroke.
- Group the controls menu under headings and add **/** to filter it.

- Add a spectrum visualizer over Local Matui's own local playback, as a panel or a
  full-screen view, cycled with **v**. Analysis uses only decoded samples this
  process is scheduled to emit; a remote speaker, disabled local audio or muted
  output shows the reason rather than invented motion.

## 0.1.0-beta.1 — 2026-09-08

First private beta for Omarchy/Arch Linux x86-64, targeting Music Assistant 2.10.2.

- Browse library and provider music, open collections, search, and choose play-now,
  play-next or add-to-queue actions for individual items or whole collections.
- Control remote speakers and expose this computer through embedded Sendspin 0.3.7.
- Configure Music Assistant using a password or profile token, with masked input,
  terminal paste support and desktop keyring persistence.
- Follow live Omarchy theme changes and expose playback, volume, grouping, source,
  sleep-timer and queue controls.
- Correct local prebuffer limits and automatically select universal-player wrappers.
- Ship a native Linux archive, Arch package, Flatpak bundle, source archive, build information,
  checksums and bundled third-party notices.

Local and remote playback are confirmed on the development laptop. Prolonged
playback, other hardware/server versions, live codec changes, server restart
during streaming, acoustic latency and multi-room synchronization need further
testing. This beta does not administer Music Assistant users, providers or DSP.
