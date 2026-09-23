import hashlib
import io
import os
import re
import shutil
import subprocess
import tarfile
from pathlib import Path
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parent.parent
PLUGIN_ADD_CMD = (
    "omarchy plugin add https://example.test/omarchy-omatalk-plugin.git --enable --yes"
)


@pytest.fixture
def site(tmp_path):
    root = tmp_path / "site"
    root.mkdir()

    def publish(source_files):
        archive = root / "omatalk-src.tar.gz"
        with tarfile.open(archive, "w:gz") as tar:
            for name, content in source_files.items():
                content = content.encode()
                info = tarfile.TarInfo(f"omatalk/{name}")
                info.size = len(content)
                tar.addfile(info, io.BytesIO(content))

    return SimpleNamespace(root=root, url="https://release.test", publish=publish)


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
  if [ "${FAKE_PLUGIN_REMOVE_FAIL:-0}" = 1 ]; then exit 1; fi
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
    (fake_bin / "curl").write_text(
        """#!/bin/sh
set -eu
printf 'curl %s\\n' "$*" >> "$FAKE_LOG"
dest=""
url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o)
      dest="$2"
      shift 2
      ;;
    https://*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done
src="$FAKE_SITE/${url#https://*/}"
if [ ! -f "$src" ]; then
  exit 22
fi
cp "$src" "$dest"
"""
    )
    (fake_bin / "hyprctl").write_text(
        """#!/bin/sh
set -eu
printf 'hyprctl %s\\n' "$*" >> "$FAKE_LOG"
"""
    )
    (fake_bin / "sleep").write_text("#!/bin/sh\nexit 0\n")
    for tool in ("pgrep", "pkill"):
        (fake_bin / tool).write_text("#!/bin/sh\nexit 1\n")
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
        "OMATALK_HOME": str(tmp_path / "omatalk"),
        "FAKE_LOG": str(log),
        "FAKE_STATE": str(state),
        "FAKE_SITE": str(site.root),
        "ASK_FROM": "/dev/stdin",
        "XDG_RUNTIME_DIR": str(tmp_path / "runtime"),
        # PIN_* rewrite install.sh's fixed constants; see pinned_installer.
        "PIN_RELEASE_BASE": site.url,
        "PIN_MODEL_BASE": f"{site.url}/models",
        "PIN_MODEL_SHA256": hashlib.sha256(model).hexdigest(),
        "PIN_VOICES_SHA256": hashlib.sha256(voices).hexdigest(),
        "PIN_PLUGIN_REPO": "https://example.test/omarchy-omatalk-plugin.git",
    }
    Path(env["HOME"]).mkdir()
    return env, state, log


def without_omarchy(env):
    fake_bin = Path(env["PATH"].split(":")[0])
    (fake_bin / "omarchy").unlink(missing_ok=True)
    for tool in (
        "curl",
        "tar",
        "sha256sum",
        "mkdir",
        "rm",
        "cp",
        "mv",
        "grep",
        "cat",
        "chmod",
        "date",
        "bash",
        "touch",
    ):
        src = shutil.which(tool)
        dest = fake_bin / tool
        if src and not dest.exists():
            dest.symlink_to(src)
    env["PATH"] = str(fake_bin)
    return env


def pinned_installer(env, pins):
    """install.sh with its fixed constants rewritten for the fake site."""
    script = (ROOT / "install.sh").read_text()
    for name, value in pins.items():
        script, count = re.subn(
            rf"^{name}=.*$", f'{name}="{value}"', script, count=1, flags=re.M
        )
        assert count == 1, name
    path = Path(env["HOME"]).parent / "install.sh"
    path.write_text(script)
    return path


def run_install(env, site, answer="", tarball_sha=None):
    if tarball_sha is None:
        tarball_sha = hashlib.sha256(
            (site.root / "omatalk-src.tar.gz").read_bytes()
        ).hexdigest()
    pins = {
        name.removeprefix("PIN_"): value
        for name, value in env.items()
        if name.startswith("PIN_")
    }
    pins["TARBALL_SHA256"] = tarball_sha
    return subprocess.run(
        ["bash", str(pinned_installer(env, pins))],
        cwd=ROOT,
        env=env,
        input=answer,
        capture_output=True,
        text=True,
    )


def run_uninstall(env, answer=""):
    return subprocess.run(
        ["bash", str(ROOT / "uninstall.sh")],
        cwd=ROOT,
        env=env,
        input=answer,
        capture_output=True,
        text=True,
    )


def bindings_file(env):
    return Path(env["HOME"]) / ".config/hypr/bindings.lua"


def model_requests(log, filename):
    return sum(
        line.startswith("curl ") and line.endswith(f"/models/{filename}")
        for line in command_log(log)
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

    first = run_install(env, site, answer="n\n")

    assert first.returncode == 0, first.stderr
    install_home = Path(env["OMATALK_HOME"])
    assert (install_home / "src/stale.py").is_file()
    assert "To bind F8" in first.stdout
    assert not (Path(env["HOME"]) / ".config/omatalk/config.toml").exists()
    assert model_requests(log, "kokoro-v1.0.fp16.onnx") == 1
    assert model_requests(log, "voices-v1.0.bin") == 1

    config = Path(env["HOME"]) / ".config/omatalk/config.toml"
    config.parent.mkdir(parents=True)
    config.write_bytes(b'voice = "bf_emma"\nspeed = 1.25\n')
    bindings = Path(env["HOME"]) / ".config/hypr/bindings.lua"
    bindings.parent.mkdir(parents=True)
    bindings.write_text('o.bind("F8", "Omatalk", "omatalk speak")\n')
    config_before = config.read_bytes()

    site.publish(make_source())
    second = run_install(env, site)

    assert second.returncode == 0, second.stderr
    assert not (install_home / "src/stale.py").exists()
    assert (install_home / "src/current.py").read_text() == "new source\n"
    assert not (install_home / "src/plugin").exists()
    assert config.read_bytes() == config_before
    assert bindings.read_text() == 'o.bind("F8", "Omatalk", "omatalk speak")\n'
    assert "To bind F8" not in second.stdout
    assert model_requests(log, "kokoro-v1.0.fp16.onnx") == 1
    assert model_requests(log, "voices-v1.0.bin") == 1

    (install_home / "models/kokoro-v1.0.fp16.onnx").write_bytes(b"corrupt")
    third = run_install(env, site)

    assert third.returncode == 0, third.stderr
    assert model_requests(log, "kokoro-v1.0.fp16.onnx") == 2
    assert (install_home / "models/kokoro-v1.0.fp16.onnx").read_bytes() == b"fake model"

    (site.root / "models/kokoro-v1.0.fp16.onnx").write_bytes(b"bad download")
    (install_home / "models/kokoro-v1.0.fp16.onnx").unlink()
    bad_download = run_install(env, site)

    assert bad_download.returncode != 0

    lines = command_log(log)
    stop = max(i for i, line in enumerate(lines) if "systemctl --user stop" in line)
    clear = max(i for i, line in enumerate(lines) if "uv venv --quiet --clear" in line)
    start = max(
        i for i, line in enumerate(lines) if "systemctl --user enable --now" in line
    )
    assert stop < clear < start
    adds = [line for line in lines if "omarchy plugin add" in line]
    assert adds == [PLUGIN_ADD_CMD]
    assert (plugin_dir(env) / ".git").is_dir()
    assert not any("omarchy plugin remove" in line for line in lines)
    assert not any("omarchy restart shell" in line for line in lines)


def test_fresh_install_adds_plugin_repo_and_never_prompts_to_restart_shell(
    site, tmp_path
):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)

    result = run_install(env, site)

    assert result.returncode == 0, result.stderr
    assert "already installed before this run" not in result.stdout
    lines = command_log(log)
    assert PLUGIN_ADD_CMD in lines
    assert not any("omarchy restart shell" in line for line in lines)
    assert (plugin_dir(env) / ".git").is_dir()


def test_git_plugin_checkout_is_left_alone(site, tmp_path):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)
    path = plugin_dir(env)
    path.mkdir(parents=True)
    (path / ".git").mkdir()
    (path / "keep.txt").write_text("store checkout\n")

    result = run_install(env, site)

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

    result = run_install(env, site)

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

    result = run_install(env, site)

    assert result.returncode == 0, result.stderr
    path = plugin_dir(env)
    assert (path / ".git").is_dir()
    assert not (path / "old.txt").exists()
    lines = command_log(log)
    assert any(
        "omarchy plugin remove zerobearing.omatalk --yes" in line for line in lines
    )
    assert PLUGIN_ADD_CMD in lines
    assert not any("omarchy restart shell" in line for line in lines)


def test_legacy_copy_is_left_when_plugin_remove_fails(site, tmp_path):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)
    seed_copy_plugin(env)
    env["FAKE_PLUGIN_REMOVE_FAIL"] = "1"

    result = run_install(env, site)

    assert result.returncode == 0, result.stderr
    path = plugin_dir(env)
    assert (path / "old.txt").is_file()
    assert not (path / ".git").exists()
    lines = command_log(log)
    assert any(
        "omarchy plugin remove zerobearing.omatalk --yes" in line for line in lines
    )
    assert not any("omarchy plugin add" in line for line in lines)


def test_installer_requires_omarchy(site, tmp_path):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)
    without_omarchy(env)

    result = run_install(env, site)

    assert result.returncode != 0
    combined = result.stdout + result.stderr
    assert "Omarchy" in combined
    assert "pacman" not in combined
    if log.exists():
        assert not any("pacman" in line for line in command_log(log))


def test_installer_tolerates_missing_unit(site, tmp_path):
    site.publish(make_source())
    env, _state, _log = fake_environment(site, tmp_path)
    env["FAKE_STOP_STATUS"] = "5"

    result = run_install(env, site)

    assert result.returncode == 0, result.stderr


def test_installer_rejects_tarball_checksum_mismatch(site, tmp_path):
    site.publish(make_source())
    env, _state, _log = fake_environment(site, tmp_path)
    old_source = Path(env["OMATALK_HOME"]) / "src/old.py"
    old_source.parent.mkdir(parents=True)
    old_source.write_text("keep me\n")

    result = run_install(env, site, tarball_sha="0" * 64)

    assert result.returncode != 0
    assert old_source.read_text() == "keep me\n"


def test_installer_does_not_replace_files_if_daemon_will_not_stop(site, tmp_path):
    site.publish(make_source())
    env, _state, _log = fake_environment(site, tmp_path)
    old_source = Path(env["OMATALK_HOME"]) / "src/old.py"
    old_source.parent.mkdir(parents=True)
    old_source.write_text("keep me\n")
    env["FAKE_STOP_STATUS"] = "1"

    result = run_install(env, site)

    assert result.returncode == 1
    assert old_source.read_text() == "keep me\n"


def test_installer_fails_if_daemon_never_becomes_ready(site, tmp_path):
    site.publish(make_source())
    env, _state, _log = fake_environment(site, tmp_path)
    env["FAKE_DAEMON_DOWN"] = "1"

    result = run_install(env, site)

    assert result.returncode != 0
    assert "Daemon did not start" in result.stdout


BIND_LINE = 'o.bind("F8", "Omatalk", "omatalk speak")'


@pytest.mark.parametrize("answer", ["", "n\n"])
def test_install_does_not_bind_f8_on_eof_or_decline(site, tmp_path, answer):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)

    result = run_install(env, site, answer=answer)

    assert result.returncode == 0, result.stderr
    assert not bindings_file(env).exists()
    assert "To bind F8" in result.stdout
    assert PLUGIN_ADD_CMD in command_log(log)
    assert not any(line.startswith("hyprctl") for line in command_log(log))


@pytest.mark.parametrize("answer", ["y\n", "\n"])
def test_install_binds_f8_on_yes_or_enter(site, tmp_path, answer):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)

    result = run_install(env, site, answer=answer)

    assert result.returncode == 0, result.stderr
    assert bindings_file(env).read_text() == f"\n{BIND_LINE}\n"
    assert "To bind F8" not in result.stdout
    assert "hyprctl reload" in command_log(log)
    assert PLUGIN_ADD_CMD in command_log(log)


def test_install_reasks_after_decline(site, tmp_path):
    site.publish(make_source())
    env, _state, _log = fake_environment(site, tmp_path)
    bindings = bindings_file(env)
    bindings.parent.mkdir(parents=True)
    bindings.write_text("-- mine\n")

    first = run_install(env, site, answer="n\n")
    second = run_install(env, site, answer="y\n")

    assert first.returncode == 0, first.stderr
    assert second.returncode == 0, second.stderr
    assert bindings.read_text() == f"-- mine\n\n{BIND_LINE}\n"


def test_install_never_takes_f8_from_another_command(site, tmp_path):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)
    bindings = bindings_file(env)
    bindings.parent.mkdir(parents=True)
    bindings.write_text('o.bind("F8", "Other", "other thing")\n')
    before = bindings.read_bytes()

    result = run_install(env, site, answer="y\n")

    assert result.returncode == 0, result.stderr
    assert bindings.read_bytes() == before
    assert "F8 is already bound" in result.stdout
    assert "To bind F8" in result.stdout
    assert not any(line.startswith("hyprctl") for line in command_log(log))


def seed_bindings(env):
    bindings = bindings_file(env)
    bindings.parent.mkdir(parents=True)
    bindings.write_text(
        "-- Omatalk: F8 speaks selection\n"
        'o.bind("F9", "Dictate", "voxtype")\n'
        'o.bind("F7", "Omatalk", "omatalk speak")\n'
    )
    return bindings


def test_uninstall_removes_omatalk_bind_on_yes(tmp_path, site):
    env, _state, log = fake_environment(site, tmp_path)
    bindings = seed_bindings(env)

    result = run_uninstall(env, answer="y\n")

    assert result.returncode == 0, result.stderr
    assert bindings.read_text() == (
        '-- Omatalk: F8 speaks selection\no.bind("F9", "Dictate", "voxtype")\n'
    )
    assert "hyprctl reload" in command_log(log)
    assert "Removed the Omatalk binding" in result.stdout
    assert "Remove the o.bind line" not in result.stdout


@pytest.mark.parametrize("answer", ["", "n\n"])
def test_uninstall_keeps_bind_on_no_or_eof(tmp_path, site, answer):
    env, _state, _log = fake_environment(site, tmp_path)
    bindings = seed_bindings(env)
    before = bindings.read_bytes()

    result = run_uninstall(env, answer=answer)

    assert result.returncode == 0, result.stderr
    assert bindings.read_bytes() == before
    assert "Remove the o.bind line" in result.stdout


def test_uninstall_skips_bind_prompt_without_omatalk_bind(tmp_path, site):
    env, _state, _log = fake_environment(site, tmp_path)
    bindings = bindings_file(env)
    bindings.parent.mkdir(parents=True)
    bindings.write_text('o.bind("F9", "Dictate", "voxtype")\n')

    result = run_uninstall(env)

    assert result.returncode == 0, result.stderr
    assert "Omatalk binding" not in result.stdout + result.stderr
    assert "o.bind line" not in result.stdout


def test_installer_ignores_environment_overrides_of_its_pins(site, tmp_path):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)
    for name in ("RELEASE_TAG", "RELEASE_BASE", "MODEL_BASE", "PLUGIN_REPO"):
        env[name] = "https://evil.test/x"
    env["TARBALL_SHA256"] = "0" * 64

    subprocess.run(
        ["bash", str(ROOT / "install.sh")],
        cwd=ROOT,
        env=env,
        input="",
        capture_output=True,
        text=True,
    )

    downloads = [line for line in command_log(log) if line.startswith("curl ")]
    assert downloads
    assert not any("evil.test" in line for line in downloads)
    release = "https://github.com/zerobearing2/omatalk/releases/download/v"
    assert release in downloads[0]


def test_every_download_is_https_only_and_bounded(site, tmp_path):
    site.publish(make_source())
    env, _state, log = fake_environment(site, tmp_path)

    result = run_install(env, site)

    assert result.returncode == 0, result.stderr
    downloads = [line for line in command_log(log) if line.startswith("curl ")]
    assert len(downloads) == 3
    for line in downloads:
        for flag in (
            "--proto =https",
            "--proto-redir =https",
            "--max-filesize ",
            "--max-time ",
            "--speed-time ",
        ):
            assert flag in line, (flag, line)


def test_install_rebinds_after_uninstall_leaves_a_comment(site, tmp_path):
    site.publish(make_source())
    env, _state, _log = fake_environment(site, tmp_path)
    bindings = bindings_file(env)
    bindings.parent.mkdir(parents=True)
    bindings.write_text(
        "-- F8 speaks selection (installed via ~/Work/omatalk/install.sh)\n"
        f"{BIND_LINE}\n"
    )

    uninstalled = run_uninstall(env, answer="y\n")
    reinstalled = run_install(env, site, answer="y\n")

    assert uninstalled.returncode == 0, uninstalled.stderr
    assert reinstalled.returncode == 0, reinstalled.stderr
    assert bindings.read_text().endswith(f"\n{BIND_LINE}\n")
    assert bindings.read_text().count(BIND_LINE) == 1


def test_installer_pins_are_not_read_from_the_environment():
    script = (ROOT / "install.sh").read_text()
    for name in (
        "RELEASE_TAG",
        "TARBALL_SHA256",
        "RELEASE_BASE",
        "PLUGIN_REPO",
        "MODEL_BASE",
        "MODEL_SHA256",
        "VOICES_SHA256",
    ):
        assert re.search(rf'^{name}="[^$]', script, re.M), name
