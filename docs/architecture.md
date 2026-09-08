# Matui architecture

## Accepted scope

Rust + Ratatui Linux TUI; Music Assistant 2.10.2 is the initial integration
target. Local playback is included from the first implementation, not a later
phase. Embed `sendspin = "=0.3.7"`; do not introduce a companion player process.
Commit Cargo.lock when changes are reviewed. Matui is MIT licensed; third-party
dependencies retain their own licenses. Release gates are in `docs/releasing.md`.

## Boundaries

- TUI: rendering, focus, text entry, user intent. Never wait for network requests
  on the rendering path. Distinguish offline fixtures from live server data.
- MA API: authenticated HTTP commands, player selection, active queue resolution,
  track search, transport/volume/seek, queue insertion and play requests.
- Local audio: authenticated WebSocket connection followed by Sendspin, decoding,
  synchronized CPAL output, server volume/mute, stream lifecycle, reconnection.
- Configuration: non-secret TOML, stable local player identity and device ID.
  Token stored in Secret Service or supplied using MATUI_TOKEN, never a command-line argument.

Selecting a remote player does not move playback or start local audio. Enabling
local audio registers an endpoint but does not issue a play command; MA may send
audio to that endpoint independently, including when a prior queue resumes.
No automatic fallback from missing explicit headphones/DAC to system speakers.

## Versioned integration references

- MA HTTP handler: https://github.com/music-assistant/server/tree/2.10.2/music_assistant/controllers/webserver
- MA Sendspin authentication: https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/webserver/sendspin_proxy.py
- Sendspin 0.3.7: https://github.com/Sendspin/sendspin-rs/tree/v0.3.7
- Audio output: https://github.com/Sendspin/sendspin-rs/blob/v0.3.7/src/audio/synced_player.rs
- Desktop integration reference (not a dependency): https://github.com/music-assistant/desktop-app/blob/main/src-tauri/src/sendspin/mod.rs

MA proxy authentication is separate from the ordinary Sendspin handshake: send
an auth message with token and client_id, require auth_ok, then hand the socket
to ProtocolClientBuilder. Do not copy the upstream player example wholesale:
the application must manage decoder replacement, volume/mute, format validation,
cancellation, audio-thread lifetime and reconnects.

The MA 2.10.2 HTTP `/api` response is the bare JSON command result, not the
WebSocket response envelope. Resolve a selected player's active queue before
queue commands; volume commands still target the player itself. Submit explicit
`replace` or `add` options for play-media requests rather than relying on defaults.

Sendspin 0.3.7's protocol router has unbounded internal receivers and SyncedPlayer
callbacks use locks. Application-level channel bounds do not fix those upstream
properties. Split inbound receivers lack ordering IDs, so drain/discard pending
audio on stream boundaries rather than decoding stale bytes in a new format.
Do not enable upstream payload tracing where it might disclose protocol data.

Check audio chunk continuity in server timestamp/sample coordinates, not local
Instants converted under changing clock-sync estimates. Local deadline order can
change after synchronization corrections; occupancy is an enqueue-time estimate,
not measured device consumption. Keep those concerns separate.

Apply delay changes based on the real player's configured delay, not the budget
cache: a stream begin or stale-clock reset can reset accounting independently of
the output. Test nonzero-to-zero resets before the first buffer as well as after
clock invalidation on an actual null-output player.

Music Assistant 2.10.2 pins aiosendspin 9.1.1. Its player buffer tracker accounts
encoded bytes and permits a 30-second duration horizon. Matui advertises 2 MiB
of encoded capacity, so a two-second/2 MiB decoded limit is incompatible: 48 kHz
stereo PCM16 can legitimately fill almost 11 seconds and expands to i32 samples.
The decoded queue therefore permits 32 MiB, 4096 chunks and a 35-second scheduling
horizon (30 seconds plus timing margin). This covers the largest advertised
96 kHz stereo format. Per-chunk size/duration and overlap checks remain bounded;
the protocol/worker handoffs retain their separate limits. These are software
buffer bounds, not a claim of measured device latency or global memory bounds.

The local playback regression reproduced rejection at PCM chunk 76 with a
500 ms initial lead, before the advertised encoded capacity was reached. Tests
cover a full PCM buffer, the compressed-audio duration horizon, memory/count
bounds and deadline reordering. A synchronized real CPAL null-output test feeds
the full PCM buffer without failure; a separate default-output fixture opens a
silent stream without sending audio frames. Worker shutdown preserves fixed,
sanitized failure details instead of overwriting them with "Audio worker stopped".

Sources: the versioned server's
`music_assistant/providers/sendspin/manifest.json` and the official aiosendspin
9.1.1 distribution's `server/audio.py` and `server/roles/player/v1.py`.

Live testing showed Music Assistant wrapping this Sendspin endpoint in a universal
player. Automatic selection matches the persistent endpoint against
`output_protocols[].output_protocol_id` as well as a direct player ID, then uses
the public wrapper ID for player/queue controls. Display names are not identity
matches, and an existing user selection is preserved. Source:
https://github.com/music-assistant/server/blob/2.10.2/music_assistant/providers/universal_player/player.py

With explicit user authorization, the installed build resumed only the existing
ungrouped local queue, remained available/playing throughout two brief tests,
and was paused afterward. The final run produced a non-silent signal measured
from Matui's own PipeWire/PulseAudio sink-input monitor on the configured desktop
output. Samples stayed in memory and were discarded; no media recording was
saved. Remote player state/control snapshots were unchanged. A restart also
verified automatic selection of the universal wrapper. Acoustic latency and
multi-room synchronization were not measured.

## Engineering safeguards

- Treat successful command submission separately from confirmed player state.
  Refresh after commands; do not blindly retry mutations after a timeout.
- Keep the active queue's identity separate from player identity (grouping).
- Reject base URLs containing credentials, query strings or fragments; never
  include credentials, response bodies or raw authentication frames in errors.
- Test with local HTTP/WebSocket fixtures, not the user's live server.
- A hardware-free test or null sink does not demonstrate audible playback.
- Treat Linux output backends and device enumeration as a runtime capability;
  fail visibly when unavailable. Do not claim bit-perfect output or sync accuracy
  until measured on actual hardware.

## Laptop controller iteration (2026-09-08)

- Retain Rust/Ratatui and the exact Sendspin 0.3.7 pin. The laptop endpoint is
  embedded in Matui, active while the application runs; no background service.
- The settings screen is limited to connection/login and the local endpoint.
  Built-in login uses POST `/auth/login` with `provider_id`, `credentials`, and
  `device_name`, then reads `token`. It uses the returned session token rather
  than creating another long-lived token on each settings save. Profile tokens
  support users whose login provider is Home Assistant/OAuth.
- Read `/info` without credentials, then authenticate `auth/me` and list players before saving. Preserve
  URL prefixes, disable redirects, bound network/keyring operations, and omit
  response bodies from errors. Never put a password/token in process arguments.
- `secret-tool` passes tokens via pipes into Secret Service, keyed by server URL
  and persistent player identity. MATUI_TOKEN overrides lookup for that run.
  Config updates are private, atomic file replacements. Settings/network work
  runs on runtime workers while the terminal remains responsive.
- Theme updates reopen the palette path every 500 ms. This laptop's installed
  `/usr/share/omarchy/bin/omarchy-theme-set` uses
  `$HOME/.local/state/omarchy/current/theme/colors.toml`; the older config path
  is a fallback. No packaged Omarchy files, theme hooks or desktop settings are
  edited. Transient missing/invalid palettes retain the last valid colors.
- Playback menus expose player transport separately from MA queue transport,
  group/source/sound-mode choices, runtime player options, sleep timers and
  queue operations. They exclude server/provider/user administration. Queue
  edits carry the displayed queue ID and compare it with fresh active-queue
  ownership before mutation. Numeric entry rejects NaN, infinity, fractions for
  integer controls and out-of-range values.
- Search aggregates playable media types, with 50 results per type. Player
  capabilities and server command failures remain visible.

Additional official integration sources inspected:

- https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/webserver/controller.py
- https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/webserver/auth.py
- https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/players/controller.py
- https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/player_queues/controller.py
- https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/music/controller.py
- https://github.com/music-assistant/client/blob/main/music_assistant_client/players.py
- https://github.com/music-assistant/models/blob/main/music_assistant_models/player.py
- https://www.music-assistant.io/player-support/sendspin/

The unversioned client/model references supplement the versioned server handlers;
server tag 2.10.2 defines command compatibility. Fixture success does not establish
compatibility with an unknown live server version.

Terminal input enables bracketed paste and disables it on normal exit, signals
and panic cleanup. Paste events insert only into the active text field, excluding
control characters without turning them into shortcuts or submissions. Oversized
pastes are rejected atomically so URLs and credentials are not silently truncated.
PTY tests cover long prefixed URLs, masked password paste, search paste and paste
mode restoration on normal quit and SIGTERM.

Failed connection tests retain masked form credentials for retry; only the
worker receives a clone. A token-only PTY fixture returns HTTP 405 once and
verifies that retry succeeds without repasting or calling the password-login
endpoint. Server-info preflight rejects HTML/dashboard URLs before API token
submission. HTTP 405 errors identify the rejected API method and advise checking
the base URL/proxy route, without echoing response bodies.

## Music selection (2026-09-08)

The default content pane is a read-only music browser. Library categories use
`music/{media_type}s/library_items` with 100-item offset pages and optional
favorite filtering. Provider browsing follows server-returned folder paths.
Album/playlist rows open track listings; artist rows open albums with a top-tracks
folder. Search results retain item/provider identity and use the same navigation.
History restores the prior cursor; generation IDs discard replies after back or
new navigation. Loading, empty, failure and retry states remain inside the pane.

Enter on a playable leaf, or P on a collection, opens a menu showing the speaker
and explicit replace/next/add queue options. Browse requests need no selected
player. Playback requests retain the chosen player, check its availability at
submission, and resolve its active group queue before sending `play_media`.
Navigation never issues playback commands. The demo uses fictional catalog data.

Official 2.10.2 command/argument references:

- https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/music/media/base.py
- https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/music/media/albums.py
- https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/music/media/artists.py
- https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/music/media/playlists.py
- https://github.com/music-assistant/server/blob/2.10.2/music_assistant/controllers/music/media/radio.py

Read-only checks against the configured server confirmed version 2.10.2/schema 65
and library/provider listing endpoints. No credentials or returned personal media
are retained in test fixtures. A live `--remote-only` PTY check verified album
listing, album tracks, back navigation and provider listing using the saved login;
radio and favorite-track endpoints also succeeded in read-only checks. The connected PTY fixture checks browse before
speaker selection, playlist drill-down, track and whole-collection queue actions,
search, and group queue routing. Audible/live playback is a separate manual check.
