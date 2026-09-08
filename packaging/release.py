#!/usr/bin/env python3
"""Bundle the verified Arch package and native binary from a clean release commit.

Run stage.py and the Arch verification container first. Only an explicit file
allowlist enters the binary archive; source comes from git archive, never cwd.
"""
import hashlib
import json
import platform
import re
import shutil
import subprocess
import tarfile
import tomllib
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STAGE = ROOT / '.tools/arch-package'


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    assert not command('git', 'status', '--porcelain'), 'Commit release changes first'
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    assert re.fullmatch(r'\d+\.\d+\.\d+-beta\.\d+', version), 'Beta versions only'
    assert (STAGE / 'VERSION').read_text().strip() == version
    binary = ROOT / 'target/release/matui'
    assert command(str(binary), '--version') == f'matui {version}'
    assert digest(binary) == digest(STAGE / 'matui'), 'Staged executable is stale'
    assert digest(ROOT / 'README.md') == digest(STAGE / 'README.md'), 'Staged README is stale'
    package_name = (STAGE / 'PACKAGE-NAME').read_text().strip()
    assert package_name == f"matui-{version.replace('-', '')}-1-x86_64.pkg.tar.zst"
    package = STAGE / package_name
    assert package.is_file(), 'Build and verify the Arch package first'
    # Ensure this package actually embeds the exact release binary.
    packaged_binary = subprocess.check_output(['tar', '-xOf', str(package), 'usr/bin/matui'])
    assert hashlib.sha256(packaged_binary).hexdigest() == digest(binary)
    flatpak_stage = ROOT / '.tools/flatpak-package'
    flatpak = flatpak_stage / f'matui-v{version}-linux-x86_64.flatpak'
    flatpak_info = json.loads((flatpak_stage / 'BUILDINFO.json').read_text())
    assert flatpak_info['version'] == version
    assert flatpak_info['binary_sha256'] == digest(binary)
    assert flatpak_info['bundle_sha256'] == digest(flatpak)
    # The installed bundle, not just its build directory, must have been verified.
    verified = json.loads((flatpak_stage / 'VERIFIED.json').read_text())
    assert verified['bundle_sha256'] == digest(flatpak)
    assert verified['binary_sha256'] == digest(binary)
    prefix = f'matui-v{version}'
    destination = ROOT / 'dist' / f'v{version}'
    destination.mkdir(parents=True, exist_ok=True)
    versions = re.findall(r'GLIBC_(\d+)\.(\d+)', command('readelf', '--version-info', str(binary)))
    manifest = {
        'version': version,
        'commit': command('git', 'rev-parse', 'HEAD'),
        'source_tree': command('git', 'rev-parse', 'HEAD^{tree}'),
        'created_utc': datetime.now(timezone.utc).isoformat(),
        'target': 'x86_64-unknown-linux-gnu',
        'build_machine': platform.machine(),
        'rustc': command('rustc', '--version'),
        'cargo': command('cargo', '--version'),
        'minimum_glibc': '.'.join(map(str, max(tuple(map(int, v)) for v in versions))),
        'cargo_lock_sha256': digest(ROOT / 'Cargo.lock'),
        'binary_sha256': digest(binary),
        'arch_package_sha256': digest(package),
        'sendspin': '0.3.7',
        'flatpak': flatpak_info,
        'notice': 'Native Linux build wrapped with Arch makepkg; not a reproducible-build or signing attestation.',
    }
    manifest_path = destination / 'BUILDINFO.json'
    manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
    archive_path = destination / f'{prefix}-linux-x86_64.tar.gz'
    with tarfile.open(archive_path, 'w:gz') as archive:
        for name in ['matui', 'matui.desktop', 'README.md', 'INSTALL.txt', 'DEVELOPMENT-STATUS', 'third-party']:
            archive.add(STAGE / name, arcname=f'{prefix}/{name}')
        archive.add(manifest_path, arcname=f'{prefix}/BUILDINFO.json')
    source_path = destination / f'{prefix}-source.tar.gz'
    subprocess.run(['git', 'archive', '--format=tar.gz', f'--prefix={prefix}/',
                    '-o', str(source_path), 'HEAD'], cwd=ROOT, check=True)
    packaged_path = destination / package_name
    shutil.copyfile(package, packaged_path)
    flatpak_path = destination / flatpak.name
    shutil.copyfile(flatpak, flatpak_path)
    assets = [archive_path, source_path, packaged_path, flatpak_path, manifest_path]
    (destination / 'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.name}\n' for p in assets))
    print(destination)
    for asset in assets:
        print(f'{asset.name}: {asset.stat().st_size} bytes')


if __name__ == '__main__':
    main()
