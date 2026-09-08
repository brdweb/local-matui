# Matui

A Linux terminal controller and local speaker for Music Assistant. Built with
Rust + Ratatui and embedded **sendspin-rs 0.3.7**; no companion player process.
Matui follows the current Omarchy theme, including changes while it is open.

## Run on this laptop

```sh
matui                 # Opens connection setup when there is no saved login
matui --setup         # Edit connection/login and local speaker settings
matui --demo          # Offline preview; never connects or opens audio
```

Press **F2** for connection settings and **? / F1** for playback controls. The
settings screen accepts either a Music Assistant built-in username/password or a
profile access token. Home Assistant/OAuth users can create a profile token in
Music Assistant and paste it in the masked token field. Passwords are never
saved; the resulting token is stored in the desktop Secret Service keyring via
`secret-tool` (`libsecret` on Arch). Unlock the desktop keyring if saving fails.
`MATUI_TOKEN` remains available as an environment override for temporary use.

Set the server URL, speaker name and audio output, then select **Test connection
and save login**. This first checks that the URL returns Music Assistant server information,
then checks authentication and player-list access before saving. Use the direct
Music Assistant base URL rather than a Home Assistant dashboard or ingress link.
A failed test retains the masked token/password so you can correct the URL and
retry without pasting credentials again. Blank credentials reuse the login for the same
server. Press Tab/Shift-Tab to move between fields, Ctrl-U to clear a text field,
and Space to toggle speaker registration or cycle output devices. Use your
terminal’s paste shortcut (Shift+Insert in this laptop’s Foot configuration).
Pasted text stays in the active field; embedded line breaks do not submit it.
Esc cancels.

New setup enables **Expose this computer as a speaker** by default. After login,
Matui registers its persistent Sendspin identity and automatically selects it
when it appears. The endpoint remains available while Matui is running. Opening
settings disconnects it until you return; quitting stops local audio. Registration
does not issue a play command, but Music Assistant can send audio to the endpoint.
Use `--remote-only` to override speaker registration, or `--local` to enable it.

Default output follows the desktop's ALSA/PipeWire routing. An explicitly selected
missing device fails visibly instead of falling back to another output. Use
`matui --list-devices` to inspect available devices. Local means the computer
running Matui; running over SSH does not move audio to your laptop.

Non-secret settings live at `$XDG_CONFIG_HOME/matui/config.toml` or
`~/.config/matui/config.toml`, with mode 0600. `--config PATH` selects another file.
Keep `player_id` unchanged to retain the same Music Assistant speaker identity.
`--init` creates a configuration without overwriting an existing one. Do not put
tokens in TOML or Git. Prefer HTTPS outside a trusted LAN; HTTP does not encrypt
credentials or audio. URL userinfo, query strings and fragments are rejected.

## Playback and player controls

| Key | Action |
| --- | --- |
| Tab | Cycle players, queue and search panes |
| Up/Down or j/k | Move highlighted row |
| Enter in players | Select a player without starting playback |
| Space | Play/pause selected player's MA queue |
| n / p | Next / previous queue item |
| s / m | Stop / mute selected player |
| + / - | Volume up/down 5 points |
| Left / Right | Seek backward/forward 10 seconds |
| / | Search; Enter submits, Esc cancels |
| a in search | Append highlighted result |
| Enter in search | **Replace queue and play highlighted result** |
| Enter in queue | Play highlighted existing queue item |
| Delete in queue | Remove highlighted item |
| Shift-J / Shift-K in queue | Move item down/up |
| ? / F1 | Open playback/player controls |
| F2 | Connection settings (temporarily disconnects local speaker) |
| r | Refresh |
| q / Ctrl-C | Quit and restore terminal |

The controls menu includes direct player transport (including external sources),
mute, power, individual/group volume, absolute seek, sleep timers, compatible
player grouping, ungrouping, source/sound-mode selection and writable player
options. Queue controls include shuffle/repeat, autoplay/crossfade when reported,
play/remove/reorder/clear, playback transfer, and audiobook/podcast playback speed.
Numeric controls and media URIs have input screens. Enter applies the selected
menu action; Esc returns. Page Up/Down and Home/End navigate long lists.

Search includes tracks, albums, artists, playlists, radio, audiobooks and podcasts
(up to 50 results per type). Entering a media URI also supports playing or queuing
provider/library items without a dedicated browser. The controls menu provides
play-next and play-immediately options for the highlighted search result.

Controls target the selected player. Group queue ownership is resolved separately
from player volume. Queue item edits retain the displayed queue identity and are
rejected if grouping changes that identity. Failed mutations are never replayed,
and commands delayed more than three seconds are dropped.

## Themes

Matui rereads Omarchy's `colors.toml` every 500 ms. It supports the current
`~/.local/state/omarchy/current/theme/` layout (including `XDG_STATE_HOME`) and the
older `~/.config/omarchy/current/theme/` layout. It retains the last valid palette
while a theme directory is being replaced. No Omarchy files or hooks are changed.
Without a palette, it uses terminal colors. `NO_COLOR` disables emitted colors.

## Build and local install

Use stable Rust, a C compiler, pkg-config and ALSA headers. On Arch the build
packages are `base-devel pkgconf alsa-lib`; login persistence also needs `libsecret`
and a running Secret Service (normally supplied by the desktop keyring).

```sh
cargo build --release --locked
install -Dm755 target/release/matui ~/.local/bin/matui
install -Dm644 packaging/matui.desktop ~/.local/share/applications/matui.desktop
matui --demo
```

The launcher entry is **Matui**. Ensure `~/.local/bin` is on your desktop PATH.
Installation does not install a service or publish a release. Minimum terminal
size is 50×16; 110×30 is recommended. `matui --demo --snapshot` prints plain text.

A private Arch package workflow is also available in `packaging/arch/README.md`.
Previously built archives do not contain later branch changes; rebuild and verify
before distributing a package. No updated package or public release is implied
by the local installation above.

## Verification and boundaries

The API integration targets Music Assistant **2.10.2**, using its versioned server
sources. Automated checks use local protocol fixtures. The local desktop keyring
round-trip and CPAL/ALSA null-output tests have also been exercised on this laptop.
**A live Music Assistant login, speaker registration and audible playback still
require validation against the user's server.** No multi-room sync claim is made.

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
uv run --with pyte python tests/terminal_smoke.py target/release/matui
uv run --with pyte python tests/terminal_smoke.py target/release/matui sigterm
uv run --with pyte python tests/connected_smoke.py target/release/matui
uv run --with pyte python tests/settings_smoke.py target/release/matui
uv run --with pyte python tests/settings_smoke.py target/release/matui token
cargo test --test audio_null --locked -- --ignored
cargo test --locked --lib -- --ignored
# Uses then deletes a disposable synthetic desktop keyring entry:
cargo test --test keyring --locked -- --ignored
```

Python/uv/pyte are test tools, not runtime dependencies. The UI polls state every
two seconds; very large queues cost additional requests. Output-device changes
restart the connection. Runtime audio volume/mute/delay changes are not persisted
across application restarts. Upstream Sendspin receivers are unbounded and audio
callbacks use locks; there is no global real-time/lock-free guarantee.

Matui does not administer users, providers, DSP or the MA server. It has no
dedicated library browser, album art or desktop media-key integration. Player
support varies; server rejections appear as command errors. Application licensing
and public-distribution review remain outstanding.

See `docs/architecture.md`, `docs/development-environment.md` and `AGENTS.md`.
