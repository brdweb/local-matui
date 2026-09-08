#!/usr/bin/env python3
"""Wrap the staged native beta in a Flatpak; no host configuration is copied."""
import hashlib
import json
import shlex
import shutil
import subprocess
import tarfile
import tomllib
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / '.tools/flatpak-package'
STAGE = ROOT / '.tools/arch-package'
APP = 'io.github.brdweb.Matui'
RUNTIME = 'org.freedesktop.Platform'
BRANCH = '25.08'
SOURCE_URL = 'https://download.gnome.org/sources/libsecret/0.21/libsecret-0.21.7.tar.xz'
SOURCE_SHA256 = '6b452e4750590a2b5617adc40026f28d2f4903de15f1250e1d1c40bfd68ed55e'
FINISH_ARGS = [
    '--command=matui', '--share=network', '--socket=pulseaudio',
    '--talk-name=org.freedesktop.secrets',
    '--filesystem=~/.local/state/omarchy/current:ro',
]


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    assert (STAGE / 'VERSION').read_text().strip() == version
    assert sha(STAGE / 'matui') == sha(ROOT / 'target/release/matui')
    assert sha(STAGE / 'LICENSE') == sha(ROOT / 'LICENSE'), 'Staged license is stale'
    WORK.mkdir(parents=True, exist_ok=True)
    build = WORK / 'build'
    if build.exists():
        shutil.rmtree(build)
    # Binary-only packaging needs the Platform, not a downloaded compiler SDK.
    run('flatpak', 'build-init', str(build), APP, RUNTIME, RUNTIME, BRANCH)
    files = build / 'files'
    (files / 'bin').mkdir(parents=True, exist_ok=True)
    shutil.copy2(STAGE / 'matui', files / 'bin/matui')
    notices = files / 'share/licenses/matui'
    shutil.copytree(STAGE / 'third-party', notices)
    shutil.copy2(STAGE / 'DEVELOPMENT-STATUS', notices / 'DEVELOPMENT-STATUS')
    shutil.copy2(STAGE / 'LICENSE', notices / 'LICENSE')
    source = WORK / 'libsecret-0.21.7.tar.xz'
    if not source.exists():
        with urllib.request.urlopen(SOURCE_URL, timeout=60) as response:
            source.write_bytes(response.read())
    assert sha(source) == SOURCE_SHA256, 'libsecret source checksum mismatch'
    # Compile the unmodified helper against host headers; use runtime shared libs.
    helper = WORK / 'helper'
    helper.mkdir(exist_ok=True)
    with tarfile.open(source) as archive:
        (helper / 'secret-tool.c').write_bytes(archive.extractfile('libsecret-0.21.7/tool/secret-tool.c').read())
        (notices / 'libsecret-COPYING').write_bytes(archive.extractfile('libsecret-0.21.7/COPYING').read())
    shutil.copy2(source, notices / source.name)
    (helper / 'config.h').write_text('#define GETTEXT_PACKAGE "libsecret"\n#define LOCALEDIR "/app/share/locale"\n')
    flags = shlex.split(run('pkg-config', '--cflags', '--libs', 'libsecret-1'))
    run('cc', '-O2', '-DSECRET_API_SUBJECT_TO_CHANGE', '-DSECRET_COMPILATION', '-I' + str(helper),
        str(helper / 'secret-tool.c'), '-o', str(files / 'bin/secret-tool'), *flags)
    applications = files / 'share/applications'
    applications.mkdir(parents=True)
    shutil.copy2(ROOT / 'packaging/flatpak' / f'{APP}.desktop', applications)
    run('desktop-file-validate', str(applications / f'{APP}.desktop'))
    docs = files / 'share/doc/matui'
    docs.mkdir(parents=True)
    shutil.copy2(ROOT / 'packaging/flatpak/README.md', docs / 'README.md')
    metadata = {
        'version': version, 'app_id': APP, 'branch': 'beta',
        'runtime': f'{RUNTIME}/x86_64/{BRANCH}',
        'runtime_commit': run('flatpak', 'info', '--show-commit', f'{RUNTIME}//{BRANCH}'),
        'binary_sha256': sha(files / 'bin/matui'),
        'secret_tool_sha256': sha(files / 'bin/secret-tool'),
        'libsecret_source_sha256': SOURCE_SHA256,
        'compiler': run('cc', '--version').splitlines()[0],
        'finish_args': FINISH_ARGS,
    }
    (docs / 'BUILDINFO.json').write_text(json.dumps(metadata, indent=2) + '\n')
    run('flatpak', 'build-finish', *FINISH_ARGS, str(build))
    assert run('flatpak', 'build', str(build), '/app/bin/matui', '--version') == f'matui {version}'
    run('flatpak', 'build-export', str(WORK / 'repo'), str(build), 'beta')
    bundle = WORK / f'matui-v{version}-linux-x86_64.flatpak'
    if bundle.exists():
        bundle.unlink()
    run('flatpak', 'build-bundle', '--runtime-repo=https://flathub.org/repo/flathub.flatpakrepo',
        str(WORK / 'repo'), str(bundle), APP, 'beta')
    metadata['bundle_sha256'] = sha(bundle)
    (WORK / 'BUILDINFO.json').write_text(json.dumps(metadata, indent=2) + '\n')
    print(bundle)


if __name__ == '__main__':
    main()
