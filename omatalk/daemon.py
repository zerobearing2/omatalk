import socket
import subprocess
import threading
import time
import traceback

from .capture import capture_clipboard, capture_primary
from .chunker import sentences
from .config import load, socket_path
from .engine import Engine
from .player import play

# The onnxruntime arena grows to fit the longest utterance and never shrinks;
# an idle recycle lets systemd hand us a fresh process instead.
IDLE_TIMEOUT = 600


class Daemon:
    def __init__(self, cfg: dict):
        self.cfg = cfg
        self.state = "idle"
        self.engine = Engine(cfg)
        self._cancel = None
        self._proc = None
        self._thread = None
        self._current_text = ""
        self._last_busy = time.monotonic()

    def _notify(self, msg: str):
        subprocess.run(
            [*self.cfg["notify"], msg],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

    def _stop_current(self):
        if self._cancel:
            self._cancel.set()
        if self._proc and self._proc.poll() is None:
            self._proc.terminate()
            try:
                self._proc.wait(timeout=1)
            except subprocess.TimeoutExpired:
                self._proc.kill()
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=10)

    def speak(self, text: str):
        self.touch()
        text = text.strip() or capture_primary(self.cfg)
        if text and self.state == "speaking" and text == self._current_text:
            self.stop()
            return
        if not text:
            if self.state == "speaking":
                self.stop()
                return
            text = capture_clipboard(self.cfg)
        if not text:
            self._notify("nothing to read")
            return
        self._stop_current()
        cancel = threading.Event()
        self._cancel = cancel
        self._current_text = text
        self.state = "speaking"
        self._thread = threading.Thread(
            target=self._run, args=(text, cancel), daemon=True
        )
        self._thread.start()

    def stop(self):
        self.touch()
        self._stop_current()
        self.state = "idle"

    def touch(self):
        self._last_busy = time.monotonic()

    def idle_seconds(self):
        return time.monotonic() - self._last_busy

    def _run(self, text: str, cancel: threading.Event):
        try:
            proc = None
            for part in sentences(text):
                if cancel.is_set():
                    return
                samples, rate = self.engine.synthesize(part)
                if cancel.is_set():
                    return
                if proc:
                    proc.wait()
                    if cancel.is_set():
                        return
                proc = play(self.cfg, samples, rate)
                self._proc = proc
            if proc:
                proc.wait()
            if not cancel.is_set():
                self.state = "idle"
        except Exception as e:
            if not cancel.is_set():
                traceback.print_exc()
                self._notify(f"error: {e}")
                self.state = "error"


def handle(daemon: Daemon, line: str) -> str:
    parts = line.split(" ", 1)
    cmd = parts[0]
    if cmd == "speak":
        daemon.speak(parts[1] if len(parts) > 1 else "")
        return "ok"
    if cmd == "stop":
        daemon.stop()
        return "ok"
    if cmd == "status":
        return daemon.state
    return "unknown command"


def serve():
    cfg = load()
    daemon = Daemon(cfg)
    path = socket_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.unlink(missing_ok=True)
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(str(path))
    server.listen(8)
    server.settimeout(1.0)
    try:
        while True:
            try:
                conn, _ = server.accept()
            except TimeoutError:
                idle = daemon.idle_seconds()
                if daemon.state != "speaking" and idle > IDLE_TIMEOUT:
                    print(f"idle {int(idle)}s; recycling", flush=True)
                    break
                continue
            try:
                with conn:
                    line = conn.makefile("r").readline()
                    if not line:
                        continue
                    reply = handle(daemon, line.strip())
                    conn.sendall((reply + "\n").encode())
            except (TimeoutError, OSError):
                continue
    finally:
        path.unlink(missing_ok=True)


if __name__ == "__main__":
    serve()
