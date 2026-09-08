# Local Matui

A Linux terminal controller and local speaker for [Music Assistant](https://www.music-assistant.io/).
Browse music, manage queues, control network speakers, or play audio on the
computer running Local Matui. Built with Rust + Ratatui and embedded
**sendspin-rs 0.3.7**; no companion player process is required.

- Browse library and provider content, search, and choose where and how to play it.
- Control transport, volume, groups, sources and queues from the terminal.
- Save connection credentials in the desktop keyring.
- Follow live Omarchy theme changes, or use terminal colors on other desktops.

Local Matui is beta software targeting **Music Assistant 2.10.2**. Other server
versions and audio devices may behave differently; see [Known limitations](#known-limitations).

## Installation

Current beta: **[v0.1.0-beta.2](https://github.com/brdweb/local-matui/releases/tag/v0.1.0-beta.2)**
Download the Flatpak bundle, Arch package or Linux x86-64 archive and
`SHA256SUMS` from the release page. In the download directory, verify the files:

```sh
sha256sum --ignore-missing -c SHA256SUMS
```

Installation instructions and third-party notices are bundled. Checksums verify
file integrity; packages are not signed. See [CHANGELOG.md](CHANGELOG.md) for release notes.

### Flatpak

```sh
flatpak install --user ./local-matui-v0.1.0-beta.2-linux-x86_64.flatpak
flatpak run io.github.brdweb.LocalMatui
```

See [Flatpak setup and permissions](packaging/flatpak/README.md). It uses a separate
profile from native Local Matui; set up your login on first launch.

### Arch Linux / Omarchy

```sh
sudo pacman -U ./local-matui-0.1.0beta.2-1-x86_64.pkg.tar.zst
local-matui
```

For other Linux distributions, use Flatpak, follow the native archive's bundled
installation instructions, or [build from source](#build-and-local-install).
Run Local Matui as your normal desktop user, not with `sudo`.

## Getting started

```sh
local-matui                 # Opens connection setup when there is no saved login
local-matui --setup         # Edit connection/login and local speaker settings
local-matui --demo          # Offline preview; never connects or opens audio
```

Press **F2** for connection settings and **? / F1** for playback controls. The
settings screen accepts either a Music Assistant built-in username/password or a
profile access token. Home Assistant/OAuth users can create a profile token in
Music Assistant and paste it in the masked token field. Passwords are never
saved; the resulting token is stored in the desktop Secret Service keyring via
`secret-tool` (`libsecret` on Arch). Unlock the desktop keyring if saving fails.
`LOCAL_MATUI_TOKEN` remains available as an environment override for temporary use;
`MATUI_TOKEN` is still read for setups configured before the rename.

Set the server URL, speaker name and audio output, then select **Test connection
and save login**. This first checks that the URL returns Music Assistant server information,
then checks authentication and player-list access before saving. Use the direct
Music Assistant base URL rather than a Home Assistant dashboard or ingress link.
A failed test retains the masked token/password so you can correct the URL and
retry without pasting credentials again. Blank credentials reuse the login for the same
server. Press Tab/Shift-Tab to move between fields, Ctrl-U to clear a text field,
and Space to toggle speaker registration or cycle output devices. Use your
terminal's paste shortcut.
Pasted text stays in the active field; embedded line breaks do not submit it.
Esc cancels.

New setup enables **Expose this computer as a speaker** by default. After login,
Local Matui registers its persistent Sendspin identity and automatically selects it
when it appears. The endpoint remains available while Local Matui is running. Opening
settings disconnects it until you return; quitting stops local audio. Registration
does not issue a play command, but Music Assistant can send audio to the endpoint.
Use `--remote-only` to override speaker registration, or `--local` to enable it.

Default output follows the desktop's ALSA/PipeWire routing. An explicitly selected
missing device fails visibly instead of falling back to another output. Use
`local-matui --list-devices` to inspect available devices. Local means the computer
running Local Matui; running over SSH does not forward audio to the SSH client.

Non-secret settings live at `$XDG_CONFIG_HOME/local-matui/config.toml` or
`~/.config/local-matui/config.toml`, with mode 0600. `--config PATH` selects another file.
A configuration left at the pre-rename `matui/config.toml` is still read while no
current one exists, and a login saved under the former keyring name is still
found; move the file into `local-matui/` to switch directories. Nothing is
copied or deleted for you.
Keep `player_id` unchanged to retain the same Music Assistant speaker identity.
`--init` creates a configuration without overwriting an existing one. Do not put
tokens in TOML or Git. Prefer HTTPS outside a trusted LAN; HTTP does not encrypt
credentials or audio. URL userinfo, query strings and fragments are rejected.

## Playback and player controls

The screen keeps the players and the queue in the left column and the music
browser or search results on the right, so the queue stays visible while you
browse. The header shows the track, transport state, volume, mute and the
queue's shuffle/repeat setting. The bottom two lines are the keys for the
focused pane and the transport keys.

Select a speaker with Enter, then browse the **Music** pane. It opens with
playlists, albums, artists, tracks, radio, favorite tracks and provider browsing.
Enter opens a collection or folder; on a track it offers **Play now (replace
queue)**, **Play next**, or **Add to queue**, naming the destination speaker.
Press **P** on an album or playlist to choose playback for the whole collection.
Browsing works before selecting a speaker and does not start playback.

| Key | Action |
| --- | --- |
| Tab / Shift-Tab | Cycle players, queue, music and search panes |
| Up/Down or j/k | Move highlighted row |
| Enter in players | Select a player without starting playback |
| Space or p | Play/pause selected player's MA queue |
| < / > | Previous / next queue item (n also moves to the next) |
| s / m | Stop / mute selected player |
| z / l | Toggle shuffle / cycle repeat off→all→one |
| + / - | Volume up/down (the server chooses the step) |
| Left / Right | Seek backward/forward 10 seconds |
| / | Search; Enter submits, Esc cancels |
| b / F3 | Open music browser |
| Enter in music/search | Open collection or choose playback for an item |
| P in music/search | Choose playback for the whole highlighted collection/item |
| a / N in music/search | Add to queue / play next |
| Backspace in music | Go back, restoring the previous selection |
| ] in music | Next library page (100 items per page) |
| F4 | Focus the queue pane |
| Esc | Close the visualizer, leave search, or go back in the browser |
| Enter in queue | Play highlighted existing queue item |
| Delete in queue | Remove highlighted item |
| Shift-J / Shift-K in queue | Move item down/up |
| v | Visualizer: spectrum panel, then full screen, then off |
| ? / F1 | Open playback/player controls |
| F2 | Connection settings (temporarily disconnects local speaker) |
| r | Reload the music listing, or refresh player/queue state in other panes |
| q / Ctrl-C | Quit and restore terminal |

The controls menu includes direct player transport (including external sources),
mute, power, individual/group volume, absolute seek, sleep timers, compatible
player grouping, ungrouping, source/sound-mode selection and writable player
options. Queue controls include shuffle/repeat, autoplay/crossfade when reported,
play/remove/reorder/clear, playback transfer, and audiobook/podcast playback speed.
Numeric controls and media URIs have input screens. Entries are grouped under
headings — playback, volume, queue, grouping, sources, player options, sleep
timer and media URIs — and **/** filters them by label or heading; Esc clears
the filter, then closes the menu. Enter applies the selected menu action.
Page Up/Down and Home/End navigate long lists.

Search includes tracks, albums, artists, playlists, radio, audiobooks and podcasts
(up to 50 results per type). Album and playlist results open their tracks; artists
open albums and a top-tracks folder. Provider folders can expose music outside
your saved library. Empty or failed listings offer back/search/retry guidance.
The controls menu also accepts media URIs for playback and queueing.

### Visualizer

**v** cycles a spectrum display: a panel in place of the browser, then a
full-screen view over the track and progress line, then off. Esc closes it.
Transport keys keep working in both.

The bars are a live analysis of the audio Local Matui itself is playing through the
local speaker. A remote speaker's audio never passes through this computer, so
there is nothing to analyze then and nothing is invented: the view says which
speaker is playing instead. Muted output reads as silence. Bar height follows
the decoded stream, not a measurement of the output device, and the display is
aligned to when Local Matui is scheduled to emit each sample — device buffering and
acoustic latency are not measured. The visualizer needs local audio enabled
(F2 settings); without it, **v** explains rather than opening an empty view.

Controls target the selected player. Group queue ownership is resolved separately
from player volume. Queue item edits retain the displayed queue identity and are
rejected if grouping changes that identity. Failed mutations are never replayed,
and commands delayed more than three seconds are dropped.

## Themes

Local Matui rereads Omarchy's `colors.toml` every 500 ms. It supports the current
`~/.local/state/omarchy/current/theme/` layout (including `XDG_STATE_HOME`) and the
older `~/.config/omarchy/current/theme/` layout. It retains the last valid palette
while a theme directory is being replaced. No Omarchy files or hooks are changed.
Without a palette, it uses terminal colors. `NO_COLOR` disables emitted colors.

## Build and local install

Use stable Rust, a C compiler, pkg-config and ALSA headers. On Arch the build
packages are `base-devel pkgconf alsa-lib`; on Debian/Ubuntu they are
`build-essential pkg-config libasound2-dev`. Native login persistence also needs
`secret-tool` (`libsecret` on Arch, `libsecret-tools` on Debian/Ubuntu) and a
running Secret Service, normally supplied by the desktop keyring.

```sh
git clone https://github.com/brdweb/local-matui.git
cd local-matui
cargo build --release --locked
install -Dm755 target/release/local-matui ~/.local/bin/local-matui
install -Dm644 packaging/local-matui.desktop ~/.local/share/applications/local-matui.desktop
local-matui --demo
```

The launcher entry is **Local Matui**. Ensure `~/.local/bin` is on your desktop PATH.
Installation does not install a service. Minimum terminal
size is 50×16; 110×30 is recommended. `local-matui --demo --snapshot` prints plain text.

Package build instructions are in [packaging/arch/README.md](packaging/arch/README.md)
and [packaging/flatpak/README.md](packaging/flatpak/README.md).
Source builds may include changes not present in the latest release.

## Verification and boundaries

Validation against Music Assistant **2.10.2** has covered login, player listing,
library/provider browsing, local speaker registration and local/remote playback.
The automated suite uses local HTTP/WebSocket fixtures and checks rendering,
input handling, queue routing, decoding and reconnect behavior. Additional opt-in
tests exercise desktop keyring storage and real CPAL output on silent devices.
These checks do not establish compatibility with every device or server version,
nor do they measure acoustic latency or multi-room synchronization.

From a source checkout, run:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
uv run --with pyte python tests/terminal_smoke.py target/release/local-matui
uv run --with pyte python tests/terminal_smoke.py target/release/local-matui sigterm
uv run --with pyte python tests/connected_smoke.py target/release/local-matui
uv run --with pyte python tests/settings_smoke.py target/release/local-matui
uv run --with pyte python tests/settings_smoke.py target/release/local-matui token
cargo test --test audio_null --locked -- --ignored  # null + silent default-output tests
cargo test --locked --lib -- --ignored  # synchronized null-output buffering/delay tests
# Uses then deletes a disposable synthetic desktop keyring entry:
cargo test --test keyring --locked -- --ignored
```

Python/uv/pyte are test tools, not runtime dependencies. The ignored tests require
the named audio devices or an unlocked desktop keyring; ordinary fixtures do not
connect to a live Music Assistant server. See [architecture notes](docs/architecture.md)
for validation evidence and [release checks](docs/releasing.md) for packaging gates.

## Known limitations

The UI polls state every
two seconds; very large queues cost additional requests. Output-device changes
restart the connection. Runtime audio volume/mute/delay changes are not persisted
across application restarts. Upstream Sendspin receivers are unbounded and audio
callbacks use locks; there is no global real-time/lock-free guarantee.

Local Matui does not administer users, providers, DSP or the MA server. It has no
album art or desktop media-key integration. Player support varies; server
rejections appear as command errors. Prolonged playback, broader hardware and
codec coverage, live server restart recovery and multi-room synchronization
need further testing.

## Contributing

Bug reports and focused pull requests are welcome. Include the Local Matui and Music
Assistant versions, Linux distribution, installation method and steps to reproduce.
Do not include tokens, passwords, private server addresses or personal media data.
For code changes, follow [AGENTS.md](AGENTS.md) and run the checks above.

## License

Local Matui is licensed under the [MIT License](LICENSE). Third-party dependencies
retain their own licenses; packaged distributions include their notices.
