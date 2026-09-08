#!/bin/bash
# Run only inside a disposable Arch container with staging mounted at /work.
set -euo pipefail
[[ -e /etc/arch-release && -d /work && "${BUILD_UID:-0}" != 0 ]]
pacman-key --init
pacman -Syu --noconfirm --needed fakeroot binutils alsa-lib gcc-libs ca-certificates python python-pyte
useradd -m -u "$BUILD_UID" builder
# Rootless Docker maps the bind mount owner to container root. Build in a
# disposable builder-owned directory, then copy just the package back as root.
mkdir -p /tmp/matui-build
cp /work/PKGBUILD /work/matui /work/README.md /work/INSTALL.txt /work/THIRD-PARTY-NOTICES.tar.gz /work/DEVELOPMENT-STATUS /tmp/matui-build/
chown -R builder:builder /tmp/matui-build
cd /tmp/matui-build
runuser -u builder -- makepkg --noconfirm --force
install -m644 matui-0.1.0-1-x86_64.pkg.tar.zst /work/
package=/work/matui-0.1.0-1-x86_64.pkg.tar.zst
pacman -Qip "$package"
# The minimal image excludes documentation. Enable it only in this disposable
# container so the package integrity check covers the shipped user guide too.
python -c 'from pathlib import Path; p=Path("/etc/pacman.conf"); p.write_text("\n".join(line for line in p.read_text().splitlines() if not line.lstrip().startswith("NoExtract"))+"\n")'
pacman -U --noconfirm "$package"
pacman -Qkk matui
matui --version
matui --demo --snapshot
matui --list-devices
runuser -u builder -- python /work/terminal_smoke.py /usr/bin/matui
runuser -u builder -- python /work/terminal_smoke.py /usr/bin/matui sigterm
runuser -u builder -- python /work/connected_smoke.py /usr/bin/matui
pacman -R --noconfirm matui
test ! -e /usr/bin/matui
printf 'ARCH PACKAGE VERIFIED: install, integrity, startup, PTY, HTTP fixtures, uninstall\n'
