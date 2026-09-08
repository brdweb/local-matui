# Matui architecture

## Accepted scope

Rust + Ratatui Linux TUI; Music Assistant 2.10.2 is the initial integration
target. Local playback is included from the first implementation, not a later
phase. Embed `sendspin = "=0.3.7"`; do not introduce a companion player process.
Commit Cargo.lock when changes are reviewed. No release or application license
has been selected yet.

## Boundaries

- TUI: rendering, focus, text entry, user intent. Never wait for network requests
  on the rendering path. Distinguish offline fixtures from live server data.
- MA API: authenticated HTTP commands, player selection, active queue resolution,
  track search, transport/volume/seek, queue insertion and play requests.
- Local audio: authenticated WebSocket connection followed by Sendspin, decoding,
  synchronized CPAL output, server volume/mute, stream lifecycle, reconnection.
- Configuration: non-secret TOML, stable local player identity and device ID.
  Token supplied separately using MATUI_TOKEN, not a command-line argument.

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
