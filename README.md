# Matui

A black-and-amber Linux terminal interface for Music Assistant, with embedded
local playback through **sendspin-rs 0.3.7**. Rust + Ratatui; no companion player
process or Python runtime is needed to run the application.

## Status

First development build, targeting Music Assistant **2.10.2**. Controller and
local audio code are implemented together. Protocol fixtures, real terminal
interaction and CPAL/ALSA null-output initialization have been exercised. **Not
yet tested against a live MA instance or physical speakers/headphones.**

Implemented:

- Player selection and now-playing information.
- Play/pause, previous/next, volume and relative seek.
- Paginated queue viewing; correct active-queue resolution for grouped players.
- Provider/library track search, append to queue, replace queue and play.
- Asynchronous requests, reconnect polling, stale-data and error indicators.
- Embedded PCM/FLAC/Opus decoding and synchronized CPAL output, authenticated
  MA Sendspin proxy, volume/mute/delay handling, network reconnect and shutdown.
- Persistent local player identity, explicit output-device selection, offline demo.

## Omarchy / Arch package

The private x86-64 development package is `matui-0.1.0-1-x86_64.pkg.tar.zst`.
After copying it and its `.sha256` file to your **local** computer:

```sh
sha256sum -c matui-0.1.0-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./matui-0.1.0-1-x86_64.pkg.tar.zst
matui --demo
```

This unsigned package declares its runtime dependencies, requires glibc >= 2.39,
and does not need Rust or a separate Sendspin executable. It does not install a
service or start playback. Run Matui as your normal desktop user, not sudo, for
your desktop's audio devices. Running it over SSH plays on the remote computer.
See `packaging/arch/INSTALL.txt` for configuration and `packaging/arch/README.md`
for package generation and verification. No public release has been published.

## Build

Install a current stable Rust toolchain, a C compiler, pkg-config and ALSA headers.
For example, on Arch install `base-devel pkgconf alsa-lib`; on Debian/Ubuntu
install `build-essential pkg-config libasound2-dev`. Use rustup for Rust.

```sh
cargo build --release --locked
./target/release/matui --help
./target/release/matui --demo
./target/release/matui --demo --snapshot
```

The demo uses labelled fictional data and never connects to a server or opens
an audio stream. The snapshot is plain text; the normal UI needs a TTY.
Minimum terminal size is 50 columns by 16 rows; 110 by 30 is more comfortable.

## Configure and connect

```sh
./target/release/matui --init
./target/release/matui --list-devices
```

`--init` creates `$XDG_CONFIG_HOME/matui/config.toml`, or
`~/.config/matui/config.toml`, with mode 0600 and a generated player ID. It never
overwrites an existing file. Use `--config /path/to/config.toml` to choose another
location. Edit `server` and, optionally, `player_name` and `device_id` in that file.
Keep the generated `player_id`: changing it creates a different MA endpoint.

Create a long-lived token in Music Assistant's profile settings. Supply it through
`MATUI_TOKEN`, **not** TOML, the command line or Git. In Bash, a silent prompt avoids
putting the token itself into shell history:

```sh
read -rsp 'Music Assistant token: ' MATUI_TOKEN; printf '\n'
export MATUI_TOKEN
./target/release/matui --local
unset MATUI_TOKEN
```

`--local` enables/registers this computer as an MA player. Select that player in
the player pane to control local playback. Starting Matui does not issue a play
command, but MA may send audio to a registered local endpoint. Leave local audio
disabled when you only want to inspect/control other players.

- `--remote-only` overrides an enabled `local_playback` configuration setting.
- `local_playback = true` remembers that you want local audio on startup.
- Without `device_id`, the platform default output is used. An explicitly selected
  missing device fails visibly rather than falling back to speakers.
- Linux output goes through CPAL's ALSA backend. PipeWire/PulseAudio desktop
  routing depends on the corresponding ALSA plugins/default-device configuration.
- Prefer HTTPS for credentials across untrusted networks. HTTP/WS sends tokens
  and audio without transport encryption. URL userinfo/query/fragment are rejected.

## Keyboard controls

| Key | Action |
| --- | --- |
| Tab | Cycle players, queue and search panes |
| Up/Down or j/k | Move highlighted row |
| Enter in players | Select an available player; does not start playback |
| Space | Play/pause selected player |
| n / p | Next / previous |
| + / - | Volume up/down 5 points |
| Left / Right | Seek backward/forward 10 seconds |
| / | Enter a track search; Enter submits, Esc cancels |
| a in search results | Append highlighted track to active queue |
| Enter in search results | **Replace active queue and play highlighted track** |
| Esc | Return to queue pane |
| r | Refresh |
| q / Ctrl-C | Quit, stop local audio and restore the terminal |

Commands target the selected MA player, not necessarily this computer. Failed
mutations are not automatically replayed. Commands waiting too long are dropped
instead of being applied unexpectedly after a connection recovers.

## Verification

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
uv run --with pyte python tests/terminal_smoke.py target/release/matui
uv run --with pyte python tests/terminal_smoke.py target/release/matui sigterm
uv run --with pyte python tests/connected_smoke.py target/release/matui
# Opt-in Linux-only test; uses a real CPAL stream on ALSA's silent null output:
cargo test --test audio_null -- --ignored
# Real-player delay-reset regression on the same silent output:
cargo test --locked --lib -- --ignored
```

Python/uv/pyte are needed only for PTY smoke tests, not for Matui. HTTP/WebSocket
fixtures bind localhost and never contact your server. Null-output tests are
ignored in the normal suite because the device is not available on every host.

## Current limitations

- Search is track-only, limited to 50 results; no dedicated album/playlist browser.
- Queue is viewable with append/replace actions; no removal/reordering UI yet.
- No shuffle/repeat UI, album art, discovery wizard or desktop media-key integration.
- API state is polled every two seconds, including queue pages; very large queues
  will cost more requests. No live event subscription or cache yet.
- Output-device changes require restarting Matui. A failed audio device/decoder
  stops local audio visibly; recovery requires a restart, not a silent reroute.
- Volume changes survive network reconnects in the running audio engine, but
  runtime volume/mute/delay changes are not saved back to TOML across launches.
- The upstream 0.3.7 router has unbounded internal receivers and audio callbacks
  take locks. Application-level bounds do not establish a global real-time or
  lock-free guarantee. Queued audio is discarded at stream boundaries because
  upstream split receivers do not expose ordering IDs.
- No claims of bit-perfect output, hardware latency or measured multi-room sync.
- A private Arch package is provided for testing, not a public release.
  Application licensing and a public-distribution review remain outstanding.

Architecture and engineering guidance: `docs/architecture.md` and `AGENTS.md`.
The isolated development environment used here is documented in
`docs/development-environment.md`.
