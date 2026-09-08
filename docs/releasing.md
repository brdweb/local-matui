# Beta releases

Releases require explicit user authorization. The current beta is
`v0.1.0-beta.2`, the first published as local-matui. `v0.1.0-beta.1` was
published under the former name and withdrawn the same day; its tag and assets
were deleted rather than rewritten. Local Matui is MIT licensed; include the
root LICENSE in all new packages alongside third-party notices.
A beta tag must not be labeled as a stable/latest release.

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
   GitHub draft prerelease with `--verify-tag --prerelease --latest=false`, upload
   only the versioned dist directory's six assets, and publish it once complete.
6. Download the hosted assets into a fresh directory, verify SHA256SUMS and the
   embedded executable version/hash, and confirm the tag, source commit, intended
   repository visibility and prerelease flag. Never claim signing or reproducible builds
   unless those checks were actually performed.

Artifacts include the native Linux archive, Arch package, Flatpak bundle, source archive,
BUILDINFO.json and SHA256SUMS. The binary archives include third-party notices
from the locked Cargo graph and Rust runtime. Personal configuration, tokens,
keyring data, media and `.tools/` must never enter release assets.

The Arch version removes Cargo's prerelease hyphen (`0.1.0beta.2`); `vercmp`
confirms it sorts before stable `0.1.0`. The package is unsigned and no AUR or
distribution-repository publication is implied. No service is deployed.

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
