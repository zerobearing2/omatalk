import subprocess
import sys
import time

from conftest import FAKES, REPO


def wait_for_socket(sock_path, proc, timeout=60):
    deadline = time.time() + timeout
    while not sock_path.exists():
        assert time.time() < deadline, "daemon did not create socket"
        if proc.poll() is not None:
            raise RuntimeError(f"daemon died: {proc.stderr}")
        time.sleep(0.1)


def send(sock_path, line: str) -> str:
    import socket as s

    client = s.socket(s.AF_UNIX, s.SOCK_STREAM)
    client.settimeout(10)
    client.connect(str(sock_path))
    client.sendall((line + "\n").encode())
    reply = client.recv(1024).decode().strip()
    client.close()
    return reply


def wait_status(sock_path, want: str, timeout: float = 20):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if send(sock_path, "status") == want:
            return
        time.sleep(0.05)
    raise AssertionError(f"status never reached {want!r}")


def wait_voice_log(log_path, count: int, timeout: float = 20):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if log_path.exists() and len(log_path.read_text().splitlines()) >= count:
            return log_path.read_text().splitlines()
        time.sleep(0.05)
    raise AssertionError(f"voice log never reached {count} lines")


def test_daemon_reloads_voice_and_speed_from_config_toml_mtime(tmp_path):
    config = tmp_path / "config.toml"
    config.write_text(
        'voice = "af_heart"\n'
        "speed = 1.0\n"
        f'capture_primary = ["{FAKES}/capture-primary"]\n'
        f'capture_clipboard = ["{FAKES}/capture-clipboard"]\n'
        f'player = ["{FAKES}/player"]\n'
        f'notify = ["{FAKES}/notify"]\n'
    )
    sock = tmp_path / "reload.sock"
    voice_log = tmp_path / "voice.log"
    env = {
        "PATH": "/usr/bin:/bin",
        "OMATALK_CONFIG": str(config),
        "OMATALK_SOCKET": str(sock),
        "OMATALK_TEST_CAPTURE_FILE": str(tmp_path / "capture.txt"),
        "OMATALK_TEST_CLIPBOARD_FILE": str(tmp_path / "clipboard.txt"),
        "OMATALK_TEST_LOG": str(tmp_path / "play.log"),
        "OMATALK_TEST_NOTIFY_LOG": str(tmp_path / "notify.log"),
        "OMATALK_TEST_TICKS_FILE": str(tmp_path / "ticks.txt"),
        "OMATALK_TEST_VOICE_LOG": str(voice_log),
        "OMATALK_PYTHON": sys.executable,
        "OMATALK_TEST_FAKE_ENGINE": "1",
    }
    (tmp_path / "ticks.txt").write_text("1")
    proc = subprocess.Popen([str(REPO / "bin" / "omatalkd")], env=env)
    try:
        wait_for_socket(sock, proc)

        assert send(sock, "speak First utterance.") == "ok"
        wait_status(sock, "idle")
        first = wait_voice_log(voice_log, 1)
        assert first[-1] == "af_heart 1.0"

        # Bypass the CLI on purpose: this seam proves the Daemon's own
        # mtime-poll reload, not the writer that config.set() uses.
        config.write_text(
            'voice = "af_bella"\n'
            "speed = 1.5\n"
            f'capture_primary = ["{FAKES}/capture-primary"]\n'
            f'capture_clipboard = ["{FAKES}/capture-clipboard"]\n'
            f'player = ["{FAKES}/player"]\n'
            f'notify = ["{FAKES}/notify"]\n'
        )

        # The reload check only runs when accept() times out with no pending
        # connection (serve()'s idle branch), so the loop must leave a gap
        # longer than CONFIG_POLL_INTERVAL between attempts — hammering the
        # socket back-to-back would never let that timeout fire.
        deadline = time.time() + 20
        lines = first
        while time.time() < deadline and lines[-1] != "af_bella 1.5":
            time.sleep(1.5)
            assert send(sock, "speak Second utterance.") == "ok"
            wait_status(sock, "idle")
            lines = wait_voice_log(voice_log, len(lines) + 1)
        assert lines[-1] == "af_bella 1.5", "daemon never picked up the config.toml change"
    finally:
        proc.terminate()
        proc.wait(timeout=10)
