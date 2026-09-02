import http.server
import json
import os
import subprocess
import sys
import threading
import tomllib
from pathlib import Path

import numpy
import pytest


ROOT = Path(__file__).resolve().parent.parent
FAKE_VOICES = ["af_heart", "af_bella", "am_test", "bf_other"]


@pytest.fixture
def installer_site(tmp_path):
    root = tmp_path / "site"
    root.mkdir()
    requests = []
    (root / "install.sh").write_text(
        "#!/bin/sh\nprintf '%s' \"$UPGRADE_VALUE\" > \"$UPGRADE_MARKER\"\n"
    )

    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=root, **kwargs)

        def do_GET(self):
            requests.append(self.path)
            super().do_GET()

        def log_message(self, format, *args):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}", requests
    finally:
        server.shutdown()
        thread.join(timeout=5)


def test_upgrade_fetches_installer_without_connecting_to_daemon(
    installer_site, tmp_path
):
    site, requests = installer_site
    marker = tmp_path / "upgrade-marker"
    env = {
        **os.environ,
        "SITE_BASE": site,
        "UPGRADE_MARKER": str(marker),
        "UPGRADE_VALUE": "inherited",
        "OMATALK_SOCKET": str(tmp_path / "missing.sock"),
    }

    result = subprocess.run(
        [sys.executable, "-m", "omatalk.cli", "upgrade"],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, result.stderr
    assert marker.read_text() == "inherited"
    assert requests and "?ts=" in requests[0]


def test_upgrade_rejects_extra_arguments(installer_site, tmp_path):
    site, requests = installer_site
    result = subprocess.run(
        [sys.executable, "-m", "omatalk.cli", "upgrade", "now"],
        cwd=ROOT,
        env={**os.environ, "SITE_BASE": site},
        capture_output=True,
        text=True,
    )

    assert result.returncode == 2
    assert result.stderr.startswith("usage: omatalk")
    assert not requests


def make_notify_environment(tmp_path):
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    (fake_bin / "notify-send").write_text(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$NOTIFY_LOG\"\n"
    )
    (fake_bin / "notify-send").chmod(0o755)
    return {
        **os.environ,
        "PATH": f"{fake_bin}:{os.environ['PATH']}",
        "OMATALK_SOCKET": str(tmp_path / "missing.sock"),
        "NOTIFY_LOG": str(tmp_path / "notify.log"),
    }


def run_cli(args, env):
    if isinstance(args, str):
        args = [args]
    return subprocess.run(
        [sys.executable, "-m", "omatalk.cli", *args],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
    )


def test_status_failure_does_not_notify(tmp_path):
    env = make_notify_environment(tmp_path)

    result = run_cli("status", env)

    assert result.returncode == 1
    assert "daemon not running" in result.stderr
    assert not Path(env["NOTIFY_LOG"]).exists()


def test_speak_failure_notifies(tmp_path):
    env = make_notify_environment(tmp_path)

    result = run_cli("speak", env)

    assert result.returncode == 1
    assert "daemon not running" in result.stderr
    assert Path(env["NOTIFY_LOG"]).exists()


@pytest.fixture
def config_environment(tmp_path):
    models = tmp_path / "models"
    models.mkdir()
    # numpy.savez appends ".npz" to a bare path/string target, so pass an
    # already-open file object to keep the real "voices-v1.0.bin" name.
    with open(models / "voices-v1.0.bin", "wb") as f:
        numpy.savez(f, **{name: numpy.zeros(1) for name in FAKE_VOICES})
    return {
        **os.environ,
        "OMATALK_CONFIG": str(tmp_path / "config.toml"),
        "OMATALK_MODELS": str(models),
        "OMATALK_SOCKET": str(tmp_path / "missing.sock"),
    }


def run_config(args, env):
    return subprocess.run(
        [sys.executable, "-m", "omatalk.cli", "config", *args],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
    )


def test_config_voices_lists_full_set_without_a_daemon(config_environment):
    result = run_config(["voices", "--json"], config_environment)

    assert result.returncode == 0
    assert json.loads(result.stdout) == sorted(FAKE_VOICES)


def test_config_voices_plain_lists_one_per_line(config_environment):
    result = run_config(["voices"], config_environment)

    assert result.returncode == 0
    assert result.stdout.splitlines() == sorted(FAKE_VOICES)


def test_config_get_json_reports_full_effective_config(config_environment):
    result = run_config(["get", "--json"], config_environment)

    assert result.returncode == 0
    cfg = json.loads(result.stdout)
    assert cfg["voice"] == "af_heart"
    assert cfg["speed"] == 1.0
    assert cfg["player"] == ["pw-cat", "-p", "--raw", "--format", "s16"]


def test_config_get_plain_reports_key_value_lines(config_environment):
    result = run_config(["get"], config_environment)

    assert result.returncode == 0
    assert "voice = af_heart" in result.stdout.splitlines()


def test_config_set_voice_accepts_valid_voice_and_persists(config_environment):
    result = run_config(["set", "voice", "af_bella"], config_environment)

    assert result.returncode == 0
    written = tomllib.loads(Path(config_environment["OMATALK_CONFIG"]).read_text())
    assert written["voice"] == "af_bella"


def test_config_set_voice_rejects_unknown_voice(config_environment):
    result = run_config(["set", "voice", "not_a_real_voice"], config_environment)

    assert result.returncode == 1
    assert "not_a_real_voice" in result.stderr
    assert not Path(config_environment["OMATALK_CONFIG"]).exists()


def test_config_set_speed_accepts_valid_value(config_environment):
    result = run_config(["set", "speed", "1.5"], config_environment)

    assert result.returncode == 0
    written = tomllib.loads(Path(config_environment["OMATALK_CONFIG"]).read_text())
    assert written["speed"] == 1.5


@pytest.mark.parametrize("value", ["0.4", "2.1"])
def test_config_set_speed_rejects_out_of_range(config_environment, value):
    result = run_config(["set", "speed", value], config_environment)

    assert result.returncode == 1
    assert "0.5" in result.stderr and "2.0" in result.stderr


def test_config_set_speed_rejects_non_numeric(config_environment):
    result = run_config(["set", "speed", "fast"], config_environment)

    assert result.returncode == 1
    assert "fast" in result.stderr


@pytest.mark.parametrize("key", ["player", "notify", "capture_primary", "lang", "bogus"])
def test_config_set_rejects_unsettable_key(config_environment, key):
    result = run_config(["set", key, "whatever"], config_environment)

    assert result.returncode == 1
    assert "not settable via config set" in result.stderr


def test_speak_voice_rejects_unknown_voice_before_touching_daemon(config_environment):
    result = run_cli(["speak", "--voice", "not_a_real_voice", "hi"], config_environment)

    assert result.returncode == 1
    assert "not_a_real_voice" in result.stderr
    # The daemon-down codepath (notify_daemon_down / "daemon not running")
    # must never run: an invalid --voice is rejected before the socket is
    # touched at all, same as `config set voice <invalid>`.
    assert "daemon not running" not in result.stderr


def test_speak_voice_valid_reaches_the_same_daemon_down_failure_as_plain_speak(
    config_environment, tmp_path
):
    fake_bin = tmp_path / "notify-bin"
    fake_bin.mkdir()
    (fake_bin / "notify-send").write_text(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$NOTIFY_LOG\"\n"
    )
    (fake_bin / "notify-send").chmod(0o755)
    env = {
        **config_environment,
        "PATH": f"{fake_bin}:{config_environment['PATH']}",
        "NOTIFY_LOG": str(tmp_path / "notify.log"),
    }

    result = run_cli(["speak", "--voice", "af_bella", "hi"], env)

    assert result.returncode == 1
    assert "daemon not running" in result.stderr
    assert Path(env["NOTIFY_LOG"]).exists()


def test_config_set_round_trip_preserves_untouched_keys(config_environment):
    Path(config_environment["OMATALK_CONFIG"]).write_text(
        'player = ["custom-player", "--flag"]\n'
    )

    assert run_config(["set", "voice", "af_bella"], config_environment).returncode == 0
    assert run_config(["set", "speed", "1.5"], config_environment).returncode == 0

    result = run_config(["get", "--json"], config_environment)
    cfg = json.loads(result.stdout)
    assert cfg["voice"] == "af_bella"
    assert cfg["speed"] == 1.5
    assert cfg["player"] == ["custom-player", "--flag"]
