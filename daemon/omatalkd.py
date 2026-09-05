import os
import socket
import subprocess
import threading
import time
import traceback

from .capture import capture_clipboard, capture_primary
from .chunker import chunks
from .config import load, socket_path
from .engine import Engine, FakeEngine
from .player import PREROLL_MS, RATE, close_stdin, feed, start

# The onnxruntime arena grows to fit the longest utterance and never shrinks;
# an idle recycle lets systemd hand us a fresh process instead.
IDLE_TIMEOUT = 600

# accept() timeout so the idle-recycle check can run.
ACCEPT_TIMEOUT = 1.0


def build_engine():
    if os.environ.get("OMATALK_TEST_FAKE_ENGINE"):
        return FakeEngine()
    return Engine()


class Daemon:
    def __init__(self, engine):
        self.state = "idle"
        self.engine = engine
        self._cancel = None
        self._proc = None
        self._thread = None
        self._current_text = ""
        self._last_busy = time.monotonic()
        self._state_condition = threading.Condition()

    def _notify(self, cfg: dict, msg: str):
        subprocess.run(
            [*cfg["notify"], msg],
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

    def speak(self, text: str, voice: str | None = None):
        self.touch()
        cfg = load()
        text = text.strip() or capture_primary(cfg)
        if text and self.state == "speaking" and text == self._current_text:
            self.stop()
            return
        if not text:
            if self.state == "speaking":
                self.stop()
                return
            text = capture_clipboard(cfg)
        if not text:
            self._notify(cfg, "nothing to read")
            return
        self._stop_current()
        cancel = threading.Event()
        self._cancel = cancel
        self._current_text = text
        self._set_state("speaking")
        self._thread = threading.Thread(
            target=self._run,
            args=(text, cancel, voice or cfg["voice"], cfg["speed"], cfg["lang"], cfg),
            daemon=True,
        )
        self._thread.start()

    def stop(self):
        self.touch()
        self._stop_current()
        self._set_state("idle")

    def touch(self):
        self._last_busy = time.monotonic()

    def idle_seconds(self):
        return time.monotonic() - self._last_busy

    def _set_state(self, state: str):
        with self._state_condition:
            self.state = state
            self._state_condition.notify_all()

    def follow(self, conn: socket.socket):
        # Snapshot and register under one condition lock so a transition
        # cannot land between the initial reply and the follower's baseline.
        with self._state_condition:
            initial = self.state
            conn.sendall((initial + "\n").encode())
            threading.Thread(
                target=self._follow_push,
                args=(conn, initial),
                daemon=True,
            ).start()

    def _follow_push(self, conn: socket.socket, last: str):
        # Streams state lines to one bar widget until it hangs up.
        try:
            while True:
                with self._state_condition:
                    self._state_condition.wait_for(lambda: self.state != last, timeout=1.0)
                    state = self.state
                if state != last:
                    conn.sendall((state + "\n").encode())
                    last = state
                elif _client_closed(conn):
                    return
        except OSError:
            pass
        finally:
            conn.close()

    def _run(self, text: str, cancel: threading.Event, voice: str, speed: float, lang: str, cfg: dict):
        proc = None
        try:
            feeder = None
            player_died = False
            # Open the sink before the first create() so A2DP/HDMI can wake
            # during synthesis; the pad is silence, not speech.
            proc = start(cfg, RATE)
            self._proc = proc
            feeder = feed(proc, [], preroll_ms=PREROLL_MS, rate=RATE)
            for part in chunks(text):
                if cancel.is_set():
                    return
                samples, rate = self.engine.synthesize(part, voice, speed, lang)
                if cancel.is_set():
                    return
                # Join the previous write so PCM is not interleaved, but
                # do not wait() the player: that would tear the device
                # down between chunks.
                if feeder is not None:
                    feeder.join()
                if cancel.is_set():
                    return
                if proc.poll() is not None:
                    player_died = True
                    break
                feeder = feed(proc, samples, rate=rate)
            if feeder is not None:
                feeder.join()
            if cancel.is_set():
                return
            if player_died:
                self._notify(cfg, "error: player exited")
                self._set_state("error")
                return
            if proc is not None:
                close_stdin(proc)
                proc.wait()
            if not cancel.is_set():
                self._set_state("idle")
        except Exception as e:
            if not cancel.is_set():
                traceback.print_exc()
                self._notify(cfg, f"error: {e}")
                self._set_state("error")
        finally:
            if proc is None:
                return
            if cancel.is_set() and self._proc is proc:
                return
            if proc.poll() is not None:
                return
            close_stdin(proc)
            proc.terminate()
            try:
                proc.wait(timeout=1)
            except subprocess.TimeoutExpired:
                proc.kill()


def handle(daemon: Daemon, line: str) -> str:
    parts = line.split(" ", 1)
    cmd = parts[0]
    if cmd == "speak":
        payload = parts[1] if len(parts) > 1 else ""
        voice = None
        # Not a new verb (see ADR-0002): a `--voice <token> ` prefix on
        # speak's own payload is a per-call override, extracted here rather
        # than in Daemon.speak so the wire format for ordinary speak stays
        # byte-for-byte unchanged.
        if payload.startswith("--voice "):
            voice, _, payload = payload[len("--voice "):].partition(" ")
        daemon.speak(payload, voice=voice)
        return "ok"
    if cmd == "stop":
        daemon.stop()
        return "ok"
    if cmd == "status":
        return daemon.state
    return "unknown command"


def _client_closed(conn: socket.socket) -> bool:
    try:
        return conn.recv(1, socket.MSG_PEEK | socket.MSG_DONTWAIT) == b""
    except BlockingIOError:
        return False
    except OSError:
        return True


def serve():
    daemon = Daemon(build_engine())
    path = socket_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.unlink(missing_ok=True)
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(str(path))
    server.listen(8)
    server.settimeout(ACCEPT_TIMEOUT)
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
                with conn.makefile("r") as reader:
                    line = reader.readline()
                if not line:
                    conn.close()
                    continue
                cmd = line.strip()
                if cmd == "follow":
                    daemon.follow(conn)
                else:
                    reply = handle(daemon, cmd)
                    conn.sendall((reply + "\n").encode())
                    conn.close()
            except (TimeoutError, OSError):
                conn.close()
                continue
    finally:
        path.unlink(missing_ok=True)


if __name__ == "__main__":
    serve()
