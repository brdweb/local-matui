"""Select and verify the native executable used by every release wrapper.

MA_TUI_CI_BUILD names an extracted GitHub build artifact. Its metadata never
supplies filesystem paths: the executable and Rust notices have fixed names.
"""
import hashlib
import json
import os
import platform
import re
import subprocess
import tomllib
from dataclasses import dataclass
from pathlib import Path

TARGET = 'x86_64-unknown-linux-gnu'
CI_FIELDS = {
    'version', 'commit', 'source_tree', 'target', 'build_machine', 'rustc',
    'cargo', 'cargo_lock_sha256', 'binary_sha256', 'repository', 'run_id',
    'run_attempt', 'run_url',
}


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(root, *args):
    return subprocess.check_output(args, cwd=root, text=True).strip()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def regular_file(path):
    require(path.is_file() and not path.is_symlink(), f'Missing or unsafe build file: {path.name}')


def validate_binary(root, binary, version):
    regular_file(binary)
    header = binary.read_bytes()[:64]
    require(len(header) == 64 and header[:6] == b'\x7fELF\x02\x01'
            and int.from_bytes(header[18:20], 'little') == 62,
            'Build executable must be a little-endian x86-64 ELF binary')
    require(os.access(binary, os.X_OK),
            'Build executable is not executable; extract the CI tar archive with file modes preserved')
    require(command(root, str(binary), '--version') == f'ma-tui {version}',
            'Build executable version does not match Cargo.toml')


def validate_runtime_notices(runtime):
    require(runtime.is_dir() and not runtime.is_symlink(), 'Missing or unsafe Rust runtime notices')
    regular_file(runtime / 'COPYRIGHT-library.html')
    licenses = runtime / 'licenses'
    require(licenses.is_dir() and not licenses.is_symlink(), 'Missing or unsafe Rust runtime licenses')
    entries = list(licenses.rglob('*'))
    require(any(entry.is_file() for entry in entries), 'Rust runtime license directory is empty')
    require(all(not entry.is_symlink() and (entry.is_dir() or entry.is_file()) for entry in entries),
            'Rust runtime licenses contain unsafe entries')


def validate_ci_metadata(metadata):
    require(isinstance(metadata, dict) and set(metadata) == CI_FIELDS,
            'CI-BUILD.json has missing or unknown metadata fields')
    for key in CI_FIELDS - {'run_id', 'run_attempt'}:
        value = metadata[key]
        require(isinstance(value, str) and value.strip() == value and value
                and len(value) <= 1024 and not any(ord(char) < 32 for char in value),
                f'Invalid CI build metadata: {key}')
    for key in ('run_id', 'run_attempt'):
        value = metadata[key]
        require((type(value) is int and value > 0)
                or (isinstance(value, str) and re.fullmatch(r'[1-9][0-9]*', value)),
                f'Invalid CI build metadata: {key}')
    for key, size in [('commit', 40), ('source_tree', 40),
                      ('cargo_lock_sha256', 64), ('binary_sha256', 64)]:
        require(re.fullmatch(r'[0-9a-f]{' + str(size) + '}', metadata[key]),
                f'Invalid CI build metadata: {key}')
    require(re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', metadata['repository']),
            'Invalid CI build metadata: repository')
    require(metadata['build_machine'] == 'x86_64', 'CI build_machine must be x86_64')
    require(metadata['run_url'] ==
            f"https://github.com/{metadata['repository']}/actions/runs/{metadata['run_id']}",
            'Invalid CI build metadata: run_url')


@dataclass(frozen=True)
class BuildInput:
    binary: Path
    rust_runtime: Path
    metadata: dict

    def check_stage(self, stage):
        require(sha256(stage / 'ma-tui') == self.metadata['binary_sha256'],
                'Staged executable does not match the selected build input')
        require(json.loads((stage / 'BUILD-INPUT.json').read_text()) == self.metadata,
                'Staged build provenance does not match the selected build input; run stage.py again')


def select_build_input(root):
    root = Path(root).resolve()
    version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
    identity = {
        'version': version,
        'commit': command(root, 'git', 'rev-parse', 'HEAD'),
        'source_tree': command(root, 'git', 'rev-parse', 'HEAD^{tree}'),
        'target': TARGET,
        'cargo_lock_sha256': sha256(root / 'Cargo.lock'),
    }
    ci_directory = os.environ.get('MA_TUI_CI_BUILD')
    if ci_directory is not None:
        require(bool(ci_directory.strip()), 'MA_TUI_CI_BUILD must name an extracted build artifact')
        artifact = Path(ci_directory).expanduser().resolve()
        metadata_file = artifact / 'CI-BUILD.json'
        regular_file(metadata_file)
        metadata = json.loads(metadata_file.read_text())
        validate_ci_metadata(metadata)
        for key, expected in identity.items():
            require(metadata[key] == expected, f'CI build {key} does not match the current checkout')
        binary = artifact / 'ma-tui'
        regular_file(binary)
        require(sha256(binary) == metadata['binary_sha256'], 'CI build executable checksum mismatch')
        validate_binary(root, binary, version)
        runtime = artifact / 'rust-runtime'
        validate_runtime_notices(runtime)
        origin = {key: metadata[key] for key in ('repository', 'run_id', 'run_attempt', 'run_url')}
        origin['kind'] = 'github-actions'
        metadata = {key: value for key, value in metadata.items() if key not in origin}
        metadata['build_origin'] = origin
    else:
        binary = root / 'target/release/ma-tui'
        validate_binary(root, binary, version)
        runtime = Path(command(root, 'rustc', '--print', 'sysroot')) / 'share/doc/rust'
        validate_runtime_notices(runtime)
        metadata = {
            **identity,
            'build_machine': platform.machine(),
            'rustc': command(root, 'rustc', '--version'),
            'cargo': command(root, 'cargo', '--version'),
            'binary_sha256': sha256(binary),
            'build_origin': {'kind': 'local'},
        }
    return BuildInput(binary, runtime, metadata)
