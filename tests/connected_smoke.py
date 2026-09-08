"""Real TUI + HTTP against a local MA-shaped fixture, never a live MA server."""
import fcntl
import codecs
import http.server
import json
import os
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
import pyte

calls = []
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_POST(self):
        assert self.path == "/api"
        assert self.headers["Authorization"] == "Bearer local-fixture"
        req = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        calls.append(req)
        cmd = req["command"]
        if cmd == "players/all":
            body = [{"player_id":"member","name":"Fixture speaker","available":True,"volume_level":30,"playback_state":"paused"}]
        elif cmd == "player_queues/get_active_queue":
            body = {"queue_id":"leader","items":1,"elapsed_time":12,"current_item":{"name":"Fixture song","duration":200}}
        elif cmd == "player_queues/items":
            body = [{"queue_item_id":"item1","name":"Fixture song","duration":200}]
        elif cmd == "music/search":
            body = {"tracks":[{"name":"Search fixture","uri":"library://track/1","artists":[{"name":"Fixture artist"}]}]}
        elif cmd == "music/playlists/library_items":
            assert req["args"] == {"limit":100,"offset":0,"order_by":"sort_name","favorite":None}
            body = [{"name":"Fixture playlist","item_id":"playlist1","provider":"library","media_type":"playlist","uri":"library://playlist/playlist1"}]
        elif cmd == "music/playlists/playlist_tracks":
            assert req["args"] == {"item_id":"playlist1","provider_instance_id_or_domain":"library"}
            body = [{"name":"Browse fixture track","uri":"library://track/browsed","media_type":"track"}]
        elif cmd == "players/cmd/stop":
            assert req["args"] == {"player_id":"member"}
            body = None
        else:
            assert cmd in ("player_queues/play_pause", "player_queues/play_media", "player_queues/delete_item"), cmd
            assert req["args"]["queue_id"] == "leader"
            body = None
        data = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Length",str(len(data)))
        self.end_headers()
        self.wfile.write(data)

server = http.server.ThreadingHTTPServer(("127.0.0.1",0),Handler)
threading.Thread(target=server.serve_forever,daemon=True).start()
master,slave=pty.openpty()
fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack("HHHH",30,110,0,0))
original=termios.tcgetattr(slave)
screen=pyte.Screen(110,30)
stream=pyte.Stream(screen)
decoder=codecs.getincrementaldecoder("utf-8")("replace")

def until(predicate):
    deadline=time.monotonic()+8
    while time.monotonic()<deadline:
        if select.select([master],[],[],0.05)[0]:
            stream.feed(decoder.decode(os.read(master,65536)))
        if predicate(): return
        if proc.poll() is not None: break
    raise AssertionError(f"fixture integration failed; exit={proc.poll()}, screen={screen.display!r}")
def visible(text):
    until(lambda:text in "\n".join(screen.display))

with tempfile.TemporaryDirectory(prefix="matui-smoke-") as tmp:
    path=os.path.join(tmp,"config.toml")
    with open(path,"w") as f:
        f.write(f'server = "http://127.0.0.1:{server.server_port}"\nplayer_id = "test-local"\nlocal_playback = true\n')
    proc=subprocess.Popen([sys.argv[1],"--config",path,"--remote-only"],stdin=slave,stdout=slave,stderr=slave,
        env=dict(os.environ,TERM="xterm-256color",MATUI_TOKEN="local-fixture"))
    try:
        visible("Fixture speaker")
        visible("Music library")
        os.write(master,b"b\r")
        visible("Fixture playlist")
        assert not any(c["command"].startswith("player_queues/") for c in calls), "browsing must work before speaker selection"
        os.write(master,b"\x7f\x1b[Z")  # Back to music home, Shift-Tab to Players.
        os.write(master,b"\r")
        visible("Fixture song")
        os.write(master,b"\r")
        visible("Fixture playlist")
        os.write(master,b"\r")
        visible("Browse fixture track")
        for index, option in enumerate(("replace","next","add")):
            os.write(master,b"\r")
            visible("Play now (replace queue)")
            visible("Fixture speaker")
            os.write(master,b"j"*index+b"\r")
            until(lambda:any(c["command"]=="player_queues/play_media" and c["args"].get("media")=="library://track/browsed" and c["args"]["option"]==option for c in calls))
        os.write(master,b"\x7fP")
        visible("Play now (replace queue)")
        os.write(master,b"jj\r")
        until(lambda:any(c["command"]=="player_queues/play_media" and c["args"].get("media")=="library://playlist/playlist1" and c["args"]["option"]=="add" for c in calls))
        os.write(master,b" ")
        until(lambda:any(c["command"]=="player_queues/play_pause" for c in calls))
        os.write(master,b"/fixture\r")
        visible("Search fixture")
        os.write(master,b"a")
        until(lambda:any(c["command"]=="player_queues/play_media" and c["args"]["option"]=="add" for c in calls))
        os.write(master,b"\r")
        visible("Play now (replace queue)")
        os.write(master,b"\r")
        until(lambda:any(c["command"]=="player_queues/play_media" and c["args"].get("media")=="library://track/1" and c["args"]["option"]=="replace" for c in calls))
        os.write(master,b"?")
        visible("Controls")
        os.write(master,b"jj\r")
        until(lambda:any(c["command"]=="players/cmd/stop" for c in calls))
        os.write(master,b"\x1b")
        visible("QUEUE")
        os.write(master,b"\x1b[3~")
        until(lambda:any(c["command"]=="player_queues/delete_item" and c["args"]["item_id_or_index"]=="item1" for c in calls))
        os.write(master,b"q")
        assert proc.wait(timeout=5)==0
        assert termios.tcgetattr(slave)==original
        print("Connected fixture smoke passed: browse before player selection, playlist tracks, all queue choices, whole collection, group routing, search, controls, terminal restoration")
    finally:
        if proc.poll() is None: proc.kill();proc.wait()
        server.shutdown()
        os.close(master);os.close(slave)
