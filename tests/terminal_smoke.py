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
    os.write(master, b"quiet night\r")
    until(b"Demo search")
    if len(sys.argv) > 2 and sys.argv[2] == "sigterm":
        proc.send_signal(signal.SIGTERM)
    else:
        os.write(master, b"q")
    assert proc.wait(timeout=5) == 0
    assert termios.tcgetattr(slave) == original, "terminal mode was not restored"
    print("PTY smoke passed: render, search text, submit, quit, terminal restoration")
finally:
    if proc.poll() is None:
        proc.kill()
        proc.wait()
    os.close(master)
    os.close(slave)
