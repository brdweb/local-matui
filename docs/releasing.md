# Releases

Releases require explicit user authorization. The current release is `v0.9.1`.
`v0.9.0` was the first as ma-tui and the first that was not a beta. `v0.1.0-beta.2` was
published as local-matui before the rename, and `v0.1.0-beta.1` under that same
former name and withdrawn the same day; its tag and assets were deleted rather
than rewritten. MA-TUI is MIT licensed; include the root LICENSE in all new
packages alongside third-party notices. A prerelease tag must not be labeled as
a stable/latest release; a release that is one may be.

1. Update Cargo.toml/Cargo.lock and CHANGELOG.md on the feature branch. Run
   `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`,
   `cargo test --all-targets --locked`, and `cargo build --release --locked`.
2. Run the release binary through the demo, connected-controller and both settings
   PTY fixtures listed in README.md. Keep live-test authorization and known gaps
   explicit; fixture results do not demonstrate acoustic latency or multi-room sync.
3. Run `python3 packaging/arch/stage.py`, then the disposable Arch package
   verification command in packaging/arch/README.md. It must install, verify,
   exercise and uninstall the package successfully before publication.
   Build `python3 packaging/flatpak/build.py`, install the resulting bundle, and
   verify sandbox startup, PTY restoration, keyring round trip, theme access and
   the silent default-output fixture. See packaging/flatpak/README.md.
4. Commit and push the release preparation, review/merge the PR into main, and
   verify that the merged source tree equals the tested feature tree. Use a clean
   main checkout for `cargo build --release --locked` and
   `python3 packaging/release.py`. The bundler checks binary/package identity.
5. Create and push an annotated `v<version>` tag at that main commit. Create a
   GitHub draft release with `--verify-tag`, adding `--prerelease --latest=false`
   for a beta and neither for a release that is not one. Upload only the
   versioned dist directory's six assets, and publish it once complete.
6. Download the hosted assets into a fresh directory, verify SHA256SUMS and the
   embedded executable version/hash, and confirm the tag, source commit, intended
   repository visibility and prerelease flag. Never claim signing or reproducible builds
   unless those checks were actually performed.

Artifacts include the native Linux archive, Arch package, Flatpak bundle, source archive,
BUILDINFO.json and SHA256SUMS. The binary archives include third-party notices
from the locked Cargo graph and Rust runtime. Personal configuration, tokens,
keyring data, media and `.tools/` must never enter release assets.

The Arch version removes Cargo's prerelease hyphen, which a release without one
does not have: `0.9.0` is used as is. The package is unsigned and no AUR or
distribution-repository publication is implied. No service is deployed. The
Flatpak application branch is `stable`; it was `beta` while the releases were,
and a ref is not upgraded across branches.

## 0.9.1 validation (2026-09-16)

A patch release for RUSTSEC-2026-0285 in rustls, which 0.9.0 shipped: the
advisory was published two days before that release and the audit caught it on
the release commit. Formatting, strict Clippy, all test targets, the five
terminal fixtures, the Arch package through install/verify/uninstall in a
disposable container, and the Flatpak through identity, sandbox, keyring, PTY,
controller and silent-audio verification were re-run on a clean checkout of the
release commit. No live server test was repeated for this release; nothing in it
changes how the server is talked to beyond the TLS library.

## 0.9.0 validation (2026-09-16)

Rust formatting, strict Clippy and all ordinary test targets passed (108 tests).
All five native terminal fixture runs passed. The Arch package passed
installation, integrity, desktop and license validation, startup, device
enumeration, PTY/controller fixtures and removal in a disposable Arch container.
The installed Flatpak passed binary/helper identity, demo/device enumeration,
native-config isolation, host theme identity, a real Secret Service round trip,
quit/SIGTERM terminal restoration, the connected controller fixture and the
silent real default-output test.

Three crates entered the graph with no license file of their own —
ratatui-termina, vtparse and wezterm-input-types, all MIT. Copies were taken
from each published crate's own recorded commit, read from its
`.cargo_vcs_info.json`, and their provenance is listed in
`packaging/arch/README.md`. The staging script fails on a dependency with no
license files, which is how they were found.

Live verification during development covered the event stream, podcast browsing
and progress, album art as sixel in foot, and local playback against Music
Assistant 2.10.2 on the development laptop. Acoustic latency, multi-room
synchronization, prolonged playback, broader codec and hardware coverage and
live server-restart recovery were not tested for this release. The sixel encoder
was checked against the specification and by eye in foot, not against other
terminals claiming sixel support.

## First beta validation (2026-09-08)

Rust formatting, strict Clippy and all ordinary test targets passed. All five
native terminal fixture runs passed (quit, SIGTERM, connected music/controls,
password settings and token settings). The Arch package passed installation,
integrity, desktop validation, startup, PTY/controller fixtures and removal in a
disposable Arch container. The installed Flatpak passed binary/helper identity,
demo/device enumeration, native-config isolation, host theme identity, a real
Secret Service round trip, quit/SIGTERM terminal restoration, the connected
controller fixture and the silent real default-output test. Flatpak tests used
synthetic credentials and local servers; live acoustic playback was previously
confirmed in the native application. The audio enumeration emits warnings for
unavailable OSS/direct ALSA outputs; its PulseAudio default stream passed.

## Second beta validation (2026-09-08)

Rust formatting, strict Clippy and all ordinary test targets passed on the
renamed tree. All five native terminal fixture runs passed. The Arch package
passed installation, integrity, desktop and license validation, startup,
PTY/controller fixtures and removal in a disposable Arch container. The
installed Flatpak passed binary/helper identity, demo/device enumeration,
native-config isolation, host theme identity, a real Secret Service round trip,
quit/SIGTERM terminal restoration, the connected controller fixture and the
silent real default-output test.

Two packaging gates were repaired to reach this point, both broken before the
rename. `verify-container.sh` never copied the LICENSE that `PKGBUILD.in` began
listing as a source, so makepkg failed; the script now copies it and checks the
installed copy. The Flatpak targeted Freedesktop Platform 25.08 while this build
host had moved to glibc 2.44, which the runtime's glibc 2.42 cannot satisfy;
the build now targets 26.08 and compares the staged executable's glibc
requirement with the runtime before building.

Live audible playback was not re-tested for this release; the visualizer and
control changes were exercised through fixtures and the offline demo only.
