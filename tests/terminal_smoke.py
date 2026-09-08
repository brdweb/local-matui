"""Exercise the real binary in a PTY; no network/audio is enabled."""
import fcntl
import codecs
import os
import pty
import select
import struct
import subprocess
import sys
import termios
import time
import pyte
import signal
from pathlib import Path

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 110, 0, 0))
original = termios.tcgetattr(slave)
env = dict(os.environ, TERM="xterm-256color")
env.pop("MATUI_TOKEN", None)
proc = subprocess.Popen([sys.argv[1], "--demo"], stdin=slave, stdout=slave, stderr=slave, env=env)
output = bytearray()
screen = pyte.Screen(110, 30)
stream = pyte.Stream(screen)
decoder = codecs.getincrementaldecoder("utf-8")("replace")

def until(marker):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if select.select([master], [], [], 0.1)[0]:
            chunk = os.read(master, 65536)
            output.extend(chunk)
            stream.feed(decoder.decode(chunk))
        if marker.decode() in "\n".join(screen.display):
            return
        if proc.poll() is not None:
            break
    raise AssertionError(f"Missing {marker!r}; exit={proc.poll()}, screen={screen.display!r}")

try:
    until(b"MATUI")
    os.write(master, b"/")
    until(b"Search:")
    # Ratatui emits incremental cell updates, not a contiguous query string.
    assert b"\x1b[?2004h" in output
    os.write(master, b"\x1b[200~quiet night\n\x1b[201~")
    until(b"quiet night")
    os.write(master, b"\r")
    until(b"Demo search")
    if len(sys.argv) > 2 and sys.argv[2] == "sigterm":
        if app_id := os.environ.get("MATUI_TEST_FLATPAK_APP"):
            # flatpak run is a launcher; signal the actual sandboxed app.
            rows = subprocess.check_output(
                ["flatpak", "ps", "--columns=application,child-pid"], text=True
            ).splitlines()
            pids = [int(row.split()[1]) for row in rows
                    if len(row.split()) == 2 and row.split()[0] == app_id]
            # child-pid may be the sandbox's init wrapper. Find the app below it.
            pending = list(pids)
            apps = []
            while pending:
                pid = pending.pop()
                try:
                    comm = Path(f"/proc/{pid}/comm").read_text().strip()
                    args = Path(f"/proc/{pid}/cmdline").read_bytes()
                    if comm == "matui" and b"--demo" in args:
                        apps.append(pid)
                    pending.extend(map(int, Path(f"/proc/{pid}/task/{pid}/children").read_text().split()))
                except FileNotFoundError:
                    pass
            assert len(apps) == 1, f"Expected one running Flatpak demo, found {apps}"
            pids = apps
            os.kill(pids[0], signal.SIGTERM)
        else:
            proc.send_signal(signal.SIGTERM)
    else:
        os.write(master, b"q")
    assert proc.wait(timeout=5) == 0
    while select.select([master], [], [], 0.1)[0]:
        output.extend(os.read(master, 65536))
    assert b"\x1b[?2004l" in output, "paste mode was not disabled"
    assert termios.tcgetattr(slave) == original, "terminal mode was not restored"
    print("PTY smoke passed: render, search text, submit, quit, terminal restoration")
finally:
    if proc.poll() is None:
        proc.kill()
        proc.wait()
    os.close(master)
    os.close(slave)
