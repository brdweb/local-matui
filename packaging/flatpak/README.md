# Flatpak beta

Download `matui-v0.1.0-beta.1-linux-x86_64.flatpak` and `SHA256SUMS` from the
private GitHub beta release, then run:

```sh
sha256sum --ignore-missing -c SHA256SUMS
flatpak install --user ./matui-v0.1.0-beta.1-linux-x86_64.flatpak
flatpak run io.github.brdweb.Matui
```

The installer obtains Freedesktop Platform 25.08 from Flathub if needed. This is
an x86-64 single-file beta bundle, not a Flathub listing or an update repository.
Install a later downloaded bundle with the same command to update. To remove:
`flatpak uninstall --user io.github.brdweb.Matui` (keeps settings by default).
The desktop entry uses your terminal emulator; the command above also works
inside an already-open terminal.

Set up the connection on first launch. Flatpak settings are separate from native
Matui at `~/.var/app/io.github.brdweb.Matui/config/matui/config.toml`. Saved login
uses the desktop Secret Service keyring. The bundled `secret-tool` helper is
built from libsecret 0.21.7; its complete upstream source and LGPL notice are
included under `/app/share/licenses/matui`. Other libraries come from the runtime.
The new profile gets its own speaker identity; close native Matui before using
the Flatpak as your laptop speaker to avoid two endpoints.

Permissions enable the network for Music Assistant, PulseAudio for desktop audio
(including PipeWire's PulseAudio compatibility service), and the Secret Service
D-Bus name for login storage. Flatpak's PulseAudio permission also permits audio
input, although Matui only opens output streams. Read-only access to
`~/.local/state/omarchy/current` follows modern Omarchy theme updates without
access to the rest of your home or native Matui configuration. Legacy/custom
Omarchy theme locations are not exposed automatically. No window-system or
full-session-bus permission is requested. The runtime's default ALSA output
routes through PulseAudio; use the desktop mixer to select the physical output.

## Build and verify

Prerequisites: native release build, `flatpak`, installed
`org.freedesktop.Platform//25.08`, C compiler, `pkg-config`, libsecret development
headers and `desktop-file-validate`. No flatpak-builder or compiler SDK is needed.

```sh
cargo build --release --locked
python3 packaging/arch/stage.py
python3 packaging/flatpak/build.py
flatpak install --user --noninteractive .tools/flatpak-package/matui-v0.1.0-beta.1-linux-x86_64.flatpak
flatpak run io.github.brdweb.Matui --version
flatpak run io.github.brdweb.Matui --demo --snapshot
flatpak run io.github.brdweb.Matui --list-devices
python3 packaging/flatpak/verify.py
```

The verifier needs Cargo, `uv`, an unlocked desktop keyring and a working
PulseAudio output. It runs local protocol/terminal fixtures and a silent real
audio stream; it never connects to your Music Assistant server. Do not run
another instance of this Flatpak during the SIGTERM fixture.

The builder stages only allowlisted binary, helper, docs and notices, verifies
its pinned source download, and records binary/helper/runtime identities. It
wraps the native binary and compiles the helper locally; it does not claim a
reproducible or signed build. The application branch is `beta`.
See [release procedure](../../docs/releasing.md) for publication checks.

Official references: [single-file bundles](https://docs.flatpak.org/en/latest/single-file-bundles.html),
[sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html),
and [libsecret source](https://download.gnome.org/sources/libsecret/0.21/).
