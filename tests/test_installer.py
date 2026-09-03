import hashlib
import http.server
import io
import os
import subprocess
import tarfile
import threading
from pathlib import Path
from types import SimpleNamespace

import pytest


ROOT = Path(__file__).resolve().parent.parent


@pytest.fixture
def site(tmp_path):
    root = tmp_path / "site"
    root.mkdir()
    requests = []

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

    def publish(source_files):
        archive = root / "omatalk-src.tar.gz"
        with tarfile.open(archive, "w:gz") as tar:
            for name, content in source_files.items():
                content = content.encode()
                info = tarfile.TarInfo(f"omatalk/{name}")
                info.size = len(content)
                tar.addfile(info, io.BytesIO(content))
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        (root / "omatalk-src.tar.gz.sha256").write_text(
            f"{digest}  omatalk-src.tar.gz\n"
        )

    try:
        yield SimpleNamespace(
            root=root,
            requests=requests,
            url=f"http://127.0.0.1:{server.server_port}",
            publish=publish,
        )
    finally:
        server.shutdown()
        thread.join(timeout=5)


def make_source(stale=False):
    files = {
        "pyproject.toml": "[project]\nname = 'omatalk'\n",
        "systemd/omatalk.service": "[Service]\nExecStart=fake\n",
        "current.py": "new source\n",
    }
    if stale:
        files["stale.py"] = "old source\n"
    return files


def fake_environment(site, tmp_path):
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    state = tmp_path / "state"
    state.mkdir()
    log = tmp_path / "commands.log"

    (fake_bin / "uv").write_text(
        """#!/bin/sh
set -eu
printf 'uv %s\\n' "$*" >> "$FAKE_LOG"
if [ "$1" = venv ]; then
  venv="$4"
  mkdir -p "$venv/bin"
  cat > "$venv/bin/omatalk" <<'EOF'
#!/bin/sh
case "${1:-}" in
  status) test -f "$FAKE_STATE/ready" ;;
  speak) printf '%s\\n' "$*" >> "$FAKE_STATE/speech" ;;
esac
EOF
  chmod +x "$venv/bin/omatalk"
fi
"""
    )
    (fake_bin / "systemctl").write_text(
        """#!/bin/sh
set -eu
printf 'systemctl %s\\n' "$*" >> "$FAKE_LOG"
case "$*" in
  --user\\ stop*)
    rm -f "$FAKE_STATE/ready"
    exit "${FAKE_STOP_STATUS:-0}"
    ;;
  --user\\ enable\\ --now*)
    if [ "${FAKE_DAEMON_DOWN:-0}" != 1 ]; then
      touch "$FAKE_STATE/ready"
    fi
    ;;
esac
"""
    )
    (fake_bin / "omarchy").write_text(
        """#!/bin/sh
set -eu
printf 'omarchy %s\\n' "$*" >> "$FAKE_LOG"
if [ "$1" = pkg ] && [ "$2" = present ]; then exit 0; fi
if [ "$1" = plugin ] && [ "$2" = add ]; then
  if [ "${FAKE_PLUGIN_ADD_FAIL:-0}" = 1 ]; then exit 1; fi
  plugin_dir="$HOME/.config/omarchy/plugins/zerobearing.omatalk"
  if [ -e "$plugin_dir" ]; then exit 1; fi
  mkdir -p "$plugin_dir/.git"
  printf '{"id":"zerobearing.omatalk"}\n' > "$plugin_dir/manifest.json"
  exit 0
fi
if [ "$1" = plugin ] && [ "$2" = remove ]; then
  rm -rf "$HOME/.config/omarchy/plugins/zerobearing.omatalk"
  exit 0
fi
if [ "$1" = plugin ] && [ "$2" = list ]; then
  if [ -f "$HOME/.config/omarchy/plugins/zerobearing.omatalk/manifest.json" ]; then
    printf '[{"id":"zerobearing.omatalk"}]\n'
  else
    printf '[]\n'
  fi
  exit 0
fi
if [ "$1" = plugin ] && [ "$2" = enable ]; then exit 0; fi
"""
    )
    (fake_bin / "omarchy-shell").write_text(
        """#!/bin/sh
set -eu
printf 'omarchy-shell %s\\n' "$*" >> "$FAKE_LOG"
"""
    )
    (fake_bin / "sleep").write_text("#!/bin/sh\nexit 0\n")
    for tool in fake_bin.iterdir():
        tool.chmod(0o755)

    model_dir = site.root / "models"
    model_dir.mkdir()
    model = b"fake model"
    voices = b"fake voices"
    (model_dir / "kokoro-v1.0.fp16.onnx").write_bytes(model)
    (model_dir / "voices-v1.0.bin").write_bytes(voices)

    env = {
        **os.environ,
        "HOME": str(tmp_path / "home"),
        "PATH": f"{fake_bin}:{os.environ['PATH']}",
        "RELEASE_BASE": site.url,
        "OMATALK_HOME": str(tmp_path / "omatalk"),
        "MODEL_BASE": f"{site.url}/models",
        "MODEL_SHA256": hashlib.sha256(model).hexdigest(),
        "VOICES_SHA256": hashlib.sha256(voices).hexdigest(),
        "FAKE_LOG": str(log),
        "FAKE_STATE": str(state),
        "ASK_FROM": "/dev/stdin",
        "PLUGIN_REPO": "https://example.test/omarchy-omatalk-plugin.git",
    }
    Path(env["HOME"]).mkdir()
    return env, state, log


def run_install(env, answer=""):
    return subprocess.run(
        ["bash", str(ROOT / "install.sh")],
        cwd=ROOT,
        env=env,
        input=answer,
        capture_output=True,
        text=True,
    )


def model_requests(site, filename):
    return sum(
        request.split("?", 1)[0] == f"/models/{filename}"
        for request in site.requests
    )


def plugin_dir(env):
    return Path(env["HOME"]) / ".config/omarchy/plugins/zerobearing.omatalk"


def command_log(log):
    return log.read_text().splitlines()


def seed_copy_plugin(env):
    path = plugin_dir(env)
    path.mkdir(parents=True)
    (path / "manifest.json").write_text('{"id": "zerobearing.omatalk"}\n')
    (path / "old.txt").write_text("legacy copy\n")
    return path


def test_reinstall_converges_and_preserves_user_files(site, tmp_path):
    site.publish(make_source(stale=True))
    env, _state, log = fake_environment(site, tmp_path)

    first = run_install(env)

    assert first.returncode == 0, first.stderr
    install_home = Path(env["OMATALK_HOME"])
    assert (install_home / "src/stale.py").is_file()
    assert "To bind F8" in first.stdout
    assert not (Path(env["HOME"]) / ".config/omatalk/config.toml").exists()
    assert model_requests(site, "kokoro-v1.0.fp16.onnx") == 1
    assert model_requests(site, "voices-v1.0.bin") == 1

    config = Path(env["HOME"]) / ".config/omatalk/config.toml"
    config.parent.mkdir(parents=True)
    config.write_bytes(b'voice = "bf_emma"\nspeed = 1.25\n')
    bindings = Path(env["HOME"]) / ".config/hypr/bindings.lua"
    bindings.parent.mkdir(parents=True)
    bindings.write_text('o.bind("F8", "Omatalk", "omatalk speak")\n')
    config_before = config.read_bytes()

    site.publish(make_source())
    second = run_install(env)

    assert second.returncode == 0, second.stderr
    assert not (install_home / "src/stale.py").exists()
    assert (install_home / "src/current.py").read_text() == "new source\n"
    assert not (install_home / "src/plugin").exists()
    assert config.read_bytes() == config_before
    assert bindings.read_text() == 'o.bind("F8", "Omatalk", "omatalk speak")\n'
    assert "To bind F8" not in second.stdout
    assert model_requests(site, "kokoro-v1.0.fp16.onnx") == 1
    assert model_requests(site, "voices-v1.0.bin") == 1

    (install_home / "models/kokoro-v1.0.fp16.onnx").write_bytes(b"corrupt")
    third = run_install(env)

    assert third.returncode == 0, third.stderr
    assert model_requests(site, "kokoro-v1.0.fp16.onnx") == 2
    assert (install_home / "models/kokoro-v1.0.fp16.onnx").read_bytes() == b"fake model"

    (site.root / "models/kokoro-v1.0.fp16.onnx").write_bytes(b"bad download")
    (install_home / "models/kokoro-v1.0.fp16.onnx").unlink()
    bad_download = run_install(env)

    assert bad_download.returncode != 0

    lines = command_log(log)
    stop = max(i for i, line in enumerate(lines) if "systemctl --user stop" in line)
    clear = max(i for i, line in enumerate(lines) if "uv venv --quiet --clear" in line)
    start = max(i for i, line in enumerate(lines) if "systemctl --user enable --now" in line)
    assert stop < clear < start
    adds = [line for line in lines if "omarchy plugin add" in line]
    assert adds == [
        "omarchy plugin add https://example.test/omarchy-omatalk-plugin.git --enable --yes"
    ]
    assert (plugin_dir(env) / ".git").is_dir()
    assert not any("omarchy plugin remove" in line for line in lines)
    assert not any("omarchy restart shell" in line for line in lines)


def test_fresh_install_adds_plugin_repo_and_never_prompts_to_restart_shell(
    site, tmp_path
):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)

    result = run_install(env)

    assert result.returncode == 0, result.stderr
    assert "already installed before this run" not in result.stdout
    lines = command_log(log)
    assert (
        "omarchy plugin add https://example.test/omarchy-omatalk-plugin.git --enable --yes"
        in lines
    )
    assert not any("omarchy restart shell" in line for line in lines)
    assert (plugin_dir(env) / ".git").is_dir()


def test_git_plugin_checkout_is_left_alone(site, tmp_path):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)
    path = plugin_dir(env)
    path.mkdir(parents=True)
    (path / ".git").mkdir()
    (path / "keep.txt").write_text("store checkout\n")

    result = run_install(env)

    assert result.returncode == 0, result.stderr
    assert (path / "keep.txt").read_text() == "store checkout\n"
    assert (path / ".git").is_dir()
    lines = command_log(log)
    assert not any("omarchy plugin add" in line for line in lines)
    assert not any("omarchy plugin remove" in line for line in lines)
    assert not any("omarchy plugin enable" in line for line in lines)
    assert not any("omarchy restart shell" in line for line in lines)


def test_plugin_add_failure_still_installs_the_daemon(site, tmp_path):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)
    env["FAKE_PLUGIN_ADD_FAIL"] = "1"

    result = run_install(env)

    assert result.returncode == 0, result.stderr
    assert Path(env["HOME"], ".local/bin/omatalk").is_file()
    assert not plugin_dir(env).exists()
    lines = command_log(log)
    assert any("omarchy plugin add" in line for line in lines)
    assert not any("omarchy plugin enable" in line for line in lines)
    assert not any("omarchy restart shell" in line for line in lines)


def test_legacy_copy_is_replaced_via_plugin_remove_and_add(site, tmp_path):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)
    seed_copy_plugin(env)

    result = run_install(env)

    assert result.returncode == 0, result.stderr
    path = plugin_dir(env)
    assert (path / ".git").is_dir()
    assert not (path / "old.txt").exists()
    lines = command_log(log)
    assert any("omarchy plugin remove zerobearing.omatalk --yes" in line for line in lines)
    assert (
        "omarchy plugin add https://example.test/omarchy-omatalk-plugin.git --enable --yes"
        in lines
    )
    assert not any("omarchy restart shell" in line for line in lines)


def test_installer_tolerates_missing_unit(site, tmp_path):
    site.publish(make_source())
    env, _state, _log = fake_environment(site, tmp_path)
    env["FAKE_STOP_STATUS"] = "5"

    result = run_install(env)

    assert result.returncode == 0, result.stderr


def test_installer_does_not_replace_files_if_daemon_will_not_stop(site, tmp_path):
    site.publish(make_source())
    env, _state, _log = fake_environment(site, tmp_path)
    old_source = Path(env["OMATALK_HOME"]) / "src/old.py"
    old_source.parent.mkdir(parents=True)
    old_source.write_text("keep me\n")
    env["FAKE_STOP_STATUS"] = "1"

    result = run_install(env)

    assert result.returncode == 1
    assert old_source.read_text() == "keep me\n"


def test_installer_fails_if_daemon_never_becomes_ready(site, tmp_path):
    site.publish(make_source())
    env, _state, _log = fake_environment(site, tmp_path)
    env["FAKE_DAEMON_DOWN"] = "1"

    result = run_install(env)

    assert result.returncode != 0
    assert "Daemon did not start" in result.stdout
