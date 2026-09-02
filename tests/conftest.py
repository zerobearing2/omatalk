import os
import subprocess
import sys
import time
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
FAKES = REPO / "tests" / "fakes"


@pytest.fixture(scope="session")
def tmp_base(tmp_path_factory):
    return tmp_path_factory.mktemp("omatalk")


@pytest.fixture(scope="session")
def config(tmp_base):
    cfg = tmp_base / "config.toml"
    cfg.write_text(
        f'capture_primary = ["{FAKES}/capture-primary"]\n'
        f'capture_clipboard = ["{FAKES}/capture-clipboard"]\n'
        f'player = ["{FAKES}/player"]\n'
        f'notify = ["{FAKES}/notify"]\n'
    )
    return cfg


@pytest.fixture(scope="session")
def daemon(config, tmp_base):
    env = {
        **os.environ,
        "OMATALK_CONFIG": str(config),
        "OMATALK_SOCKET": str(tmp_base / "d.sock"),
        "OMATALK_TEST_CAPTURE_FILE": str(tmp_base / "capture.txt"),
        "OMATALK_TEST_CLIPBOARD_FILE": str(tmp_base / "clipboard.txt"),
        "OMATALK_TEST_LOG": str(tmp_base / "play.log"),
        "OMATALK_TEST_NOTIFY_LOG": str(tmp_base / "notify.log"),
        "OMATALK_TEST_TICKS_FILE": str(tmp_base / "ticks.txt"),
        "OMATALK_TEST_VOICE_LOG": str(tmp_base / "voice.log"),
        "OMATALK_PYTHON": sys.executable,
        "OMATALK_TEST_FAKE_ENGINE": "1",
    }
    (tmp_base / "ticks.txt").write_text("1")
    proc = subprocess.Popen(
        [str(REPO / "bin" / "omatalkd")], env=env
    )
    sock = tmp_base / "d.sock"
    deadline = time.time() + 60
    while not sock.exists():
        assert time.time() < deadline, "daemon did not create socket"
        if proc.poll() is not None:
            raise RuntimeError(f"daemon died: {proc.stderr}")
        time.sleep(0.1)
    yield {"env": env, "tmp": tmp_base, "proc": proc}
    proc.terminate()
    proc.wait(timeout=10)


def send(daemon, line: str, sock: str = "d.sock") -> str:
    import socket as s

    client = s.socket(s.AF_UNIX, s.SOCK_STREAM)
    client.settimeout(10)
    client.connect(str(daemon["tmp"] / sock))
    client.sendall((line + "\n").encode())
    reply = client.recv(1024).decode().strip()
    client.close()
    return reply


def wait_status(daemon, want: str, timeout: float = 20, sock: str = "d.sock"):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if send(daemon, "status", sock=sock) == want:
            return
        time.sleep(0.05)
    raise AssertionError(f"status never reached {want!r}")


def wait_log(daemon, prefix: str, count: int = 1, timeout: float = 20, filename: str = "play.log"):
    deadline = time.time() + timeout
    while time.time() < deadline:
        lines = log(daemon, filename).splitlines()
        if len([l for l in lines if l.startswith(prefix)]) >= count:
            return lines
        time.sleep(0.05)
    raise AssertionError(f"{filename} never reached {count} {prefix!r} lines")


def log(daemon, filename: str = "play.log") -> str:
    return (daemon["tmp"] / filename).read_text()


def set_play_ticks(daemon, ticks: str):
    (daemon["tmp"] / "ticks.txt").write_text(ticks)


def set_capture(daemon, text: str):
    (daemon["tmp"] / "capture.txt").write_text(text)
    (daemon["tmp"] / "clipboard.txt").write_text("")


def clear_logs(daemon):
    for name in ("play.log", "notify.log"):
        path = daemon["tmp"] / name
        path.write_text("")
