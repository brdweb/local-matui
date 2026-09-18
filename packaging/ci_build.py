#!/usr/bin/env python3
"""Collect the main-branch GitHub release build using an explicit file allowlist.

The tar wrapper preserves the executable mode when GitHub stores the artifact.
Rust runtime notices come from the same toolchain that built the executable.
This metadata identifies a build; it is not a signing or reproducibility claim.
"""
import argparse
import hashlib
import io
import json
import os
import platform
import re
import subprocess
import tarfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = 'x86_64-unknown-linux-gnu'


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def collect(destination):
    require(not command('git', 'status', '--porcelain'), 'Release source must be clean')
    require(os.environ.get('GITHUB_REF') == 'refs/heads/main', 'Release builds require main')
    require(os.environ.get('GITHUB_EVENT_NAME') in ('push', 'workflow_dispatch'),
            'Release builds require a main push or manual workflow dispatch')
    commit = command('git', 'rev-parse', 'HEAD')
    require(os.environ.get('GITHUB_SHA') == commit, 'GITHUB_SHA does not match HEAD')
    repository = os.environ.get('GITHUB_REPOSITORY', '')
    require(re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository),
            'Missing or invalid GITHUB_REPOSITORY')
    require(os.environ.get('GITHUB_SERVER_URL') == 'https://github.com',
            'Expected the GitHub.com build server')
    origin = command('git', 'remote', 'get-url', 'origin')
    require(origin in (f'https://github.com/{repository}',
                       f'https://github.com/{repository}.git',
                       f'git@github.com:{repository}.git'),
            'Git origin does not match GITHUB_REPOSITORY')
    run_id = os.environ.get('GITHUB_RUN_ID', '')
    run_attempt = os.environ.get('GITHUB_RUN_ATTEMPT', '')
    require(re.fullmatch(r'[1-9][0-9]*', run_id), 'Missing or invalid GITHUB_RUN_ID')
    require(re.fullmatch(r'[1-9][0-9]*', run_attempt), 'Missing or invalid GITHUB_RUN_ATTEMPT')
    require(platform.system() == 'Linux' and platform.machine() == 'x86_64',
            'The release build must run on Linux x86_64')
    require(f'host: {TARGET}' in command('rustc', '--version', '--verbose').splitlines(),
            'Unexpected Rust host target')

    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    binary = ROOT / 'target/release/ma-tui'
    require(binary.is_file() and not binary.is_symlink(), 'Missing release executable')
    require(binary.stat().st_mode & 0o111, 'Release executable has no executable permission')
    require(command(str(binary), '--version') == f'ma-tui {version}',
            'Release executable version does not match Cargo.toml')
    rust_docs = Path(command('rustc', '--print', 'sysroot')) / 'share/doc/rust'
    licenses = rust_docs / 'licenses'
    require(licenses.is_dir() and not licenses.is_symlink(), 'Missing Rust runtime licenses')
    license_paths = sorted(licenses.rglob('*'))
    require(any(path.is_file() for path in license_paths), 'Rust runtime licenses are empty')
    require(all(not path.is_symlink() and (path.is_file() or path.is_dir())
                for path in license_paths), 'Unexpected entry in Rust runtime licenses')
    files = [('ma-tui', binary),
             ('rust-runtime/COPYRIGHT-library.html', rust_docs / 'COPYRIGHT-library.html')]
    files.extend((f'rust-runtime/{path.relative_to(rust_docs).as_posix()}', path)
                 for path in license_paths if path.is_file())
    for name, path in files:
        require(path.is_file() and not path.is_symlink(), f'Missing or unsafe artifact input: {name}')

    metadata = {
        'version': version,
        'commit': commit,
        'source_tree': command('git', 'rev-parse', 'HEAD^{tree}'),
        'target': TARGET,
        'build_machine': platform.machine(),
        'rustc': command('rustc', '--version'),
        'cargo': command('cargo', '--version'),
        'cargo_lock_sha256': digest(ROOT / 'Cargo.lock'),
        'binary_sha256': digest(binary),
        'repository': repository,
        'run_id': run_id,
        'run_attempt': run_attempt,
        'run_url': f'https://github.com/{repository}/actions/runs/{run_id}',
    }
    metadata_bytes = (json.dumps(metadata, indent=2) + '\n').encode()
    timestamp = int(command('git', 'show', '-s', '--format=%ct', 'HEAD'))
    destination.mkdir(parents=True, exist_ok=True)
    archive_path = destination / 'ma-tui-release-build.tar.gz'
    with tarfile.open(archive_path, 'w:gz') as archive:
        for name, path in files:
            info = archive.gettarinfo(str(path), arcname=name)
            info.uid = info.gid = 0
            info.uname = info.gname = ''
            info.mtime = timestamp
            info.mode &= 0o777
            with path.open('rb') as source:
                archive.addfile(info, source)
        info = tarfile.TarInfo('CI-BUILD.json')
        info.size = len(metadata_bytes)
        info.mode = 0o644
        info.mtime = timestamp
        archive.addfile(info, io.BytesIO(metadata_bytes))
    (destination / 'SHA256SUMS').write_text(f'{digest(archive_path)}  {archive_path.name}\n')
    print(archive_path)
    print(f'Executable SHA-256: {metadata["binary_sha256"]}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'dist/ci-build')
    args = parser.parse_args()
    collect(args.output.resolve())


if __name__ == '__main__':
    main()
