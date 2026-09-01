import http.server
import os
import subprocess
import sys
import threading
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parent.parent


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


def run_cli(command, env):
    return subprocess.run(
        [sys.executable, "-m", "omatalk.cli", command],
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
