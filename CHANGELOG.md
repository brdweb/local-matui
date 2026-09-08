# Changelog

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
