#!/usr/bin/env python3
"""Verify an installed beta; uses only synthetic credentials and local fixtures."""
import hashlib
import json
import os
import stat
import subprocess
import tempfile
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / '.tools/flatpak-package'
APP = 'io.github.brdweb.Matui'


def run(*args, **kwargs):
    return subprocess.check_output(args, cwd=ROOT, text=True, **kwargs).strip()


def sandbox(*args, **kwargs):
    return run('flatpak', 'run', '--user', '--command=' + args[0], APP, *args[1:], **kwargs)


def main():
    info = json.loads((WORK / 'BUILDINFO.json').read_text())
    bundle = WORK / f"matui-v{info['version']}-linux-x86_64.flatpak"
    assert hashlib.sha256(bundle.read_bytes()).hexdigest() == info['bundle_sha256']
    assert sandbox('sha256sum', '/app/bin/matui').split()[0] == info['binary_sha256']
    assert sandbox('sha256sum', '/app/bin/secret-tool').split()[0] == info['secret_tool_sha256']
    assert sandbox('matui', '--version') == f"matui {info['version']}"
    assert 'MATUI' in sandbox('matui', '--demo', '--snapshot')
    assert 'default' in sandbox('matui', '--list-devices').lower()
    sandbox('sh', '-c', 'test ! -e "$HOME/.config/matui/config.toml"')
    theme = Path.home() / '.local/state/omarchy/current/theme/colors.toml'
    if theme.is_file():
        assert sandbox('sha256sum', str(theme)).split()[0] == hashlib.sha256(theme.read_bytes()).hexdigest()
    # Only our uniquely named disposable item is created/read/deleted.
    item = 'matui-flatpak-fixture-' + uuid.uuid4().hex
    value = uuid.uuid4().hex
    try:
        sandbox('secret-tool', 'store', '--label=Matui Flatpak disposable test',
                'application', item, input=value + '\n')
        assert sandbox('secret-tool', 'lookup', 'application', item) == value
    finally:
        sandbox('secret-tool', 'clear', 'application', item)
    with tempfile.TemporaryDirectory(prefix='matui-flatpak-verify-') as tmp:
        wrapper = Path(tmp) / 'matui-flatpak'
        # The connected fixture's temporary TOML needs a read-only test grant.
        # Production metadata never grants /tmp or the host configuration.
        wrapper.write_text('#!/bin/bash\nset -e\nextra=()\nif [[ ${1:-} == --config ]]; then extra+=("--filesystem=$(dirname "$2"):ro"); fi\nexec flatpak run --user "${extra[@]}" io.github.brdweb.Matui "$@"\n')
        wrapper.chmod(stat.S_IRUSR | stat.S_IWUSR | stat.S_IXUSR)
        for script, extra in [('terminal_smoke.py', []), ('terminal_smoke.py', ['sigterm']),
                              ('connected_smoke.py', [])]:
            subprocess.run(['uv', 'run', '--with', 'pyte', 'python', 'tests/' + script,
                            str(wrapper), *extra], cwd=ROOT, check=True,
                           env=dict(os.environ, MATUI_TEST_FLATPAK_APP=APP))
    # Existing real CPAL test connects to an in-process fake audio server. It
    # opens the desktop output and acknowledges commands without audio frames.
    result = run('cargo', 'test', '--test', 'audio_null', '--no-run', '--locked', '--message-format=json')
    binaries = [json.loads(line).get('executable') for line in result.splitlines()
                if line.startswith('{') and json.loads(line).get('reason') == 'compiler-artifact']
    binary = next(Path(path) for path in binaries if path)
    print(run('flatpak', 'run', '--user', '--filesystem=' + str(binary) + ':ro',
              '--command=' + str(binary), APP,
              'opens_default_output_silently_and_acknowledges_volume', '--ignored', '--exact'))
    (WORK / 'VERIFIED.json').write_text(json.dumps({
        'bundle_sha256': info['bundle_sha256'], 'binary_sha256': info['binary_sha256'],
        'checks': ['installed binary/helper identity', 'version/demo/devices',
                   'native config isolation', 'available Omarchy theme identity',
                   'real keyring round trip', 'PTY quit and SIGTERM',
                   'local HTTP music/control fixture', 'silent real default audio output'],
    }, indent=2) + '\n')
    print('FLATPAK VERIFIED: installed identity, sandbox, keyring, PTY, controller, silent audio')


if __name__ == '__main__':
    main()
