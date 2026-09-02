import json
import os
import select
import shutil
import socket
import subprocess
import time
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parent.parent
PLUGIN = ROOT / "plugin"
FAKE_PANEL_VOICES = ["af_test_one", "bf_test_two"]
FAKE_PANEL_CONFIG = {"voice": "af_test_one", "speed": 1.25}


def output_until(process, marker: str, timeout: float = 10):
    deadline = time.monotonic() + timeout
    lines = []
    while time.monotonic() < deadline:
        if process.poll() is not None:
            break
        ready, _, _ = select.select([process.stdout], [], [], 0.1)
        if not ready:
            continue
        line = process.stdout.readline()
        lines.append(line)
        if marker in line:
            return lines
    raise AssertionError(f"Quickshell output did not contain {marker!r}: {lines}")


def test_manifest_points_to_bar_widget():
    manifest = json.loads((PLUGIN / "manifest.json").read_text())

    assert manifest["kinds"] == ["bar-widget"]
    assert manifest["entryPoints"]["barWidget"] == "BarWidget.qml"
    assert (PLUGIN / manifest["entryPoints"]["barWidget"]).is_file()
    assert (PLUGIN / "Panel.qml").is_file()


@pytest.mark.parametrize("vertical", [False, True])
@pytest.mark.parametrize("initially_available", [False, True])
def test_megaphone_socket_widget_round_trip(
    tmp_path, vertical, initially_available
):
    quickshell = shutil.which("quickshell")
    shell_dir = Path("/usr/share/omarchy/shell")
    if not quickshell or not (shell_dir / "Ui/qmldir").is_file():
        pytest.skip("requires an installed Omarchy Quickshell shell")
    if not os.environ.get("WAYLAND_DISPLAY"):
        pytest.skip("requires a graphical Wayland session")

    imports = tmp_path / "imports" / "qs"
    imports.mkdir(parents=True)
    for module in ("Ui", "Commons"):
        (imports / module).symlink_to(shell_dir / module, target_is_directory=True)

    socket_path = tmp_path / "omatalk.sock"
    server = None
    if initially_available:
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        server.bind(str(socket_path))
        server.listen(1)
        server.settimeout(10)

    shell = tmp_path / "shell.qml"
    shell.write_text(
        f'''import QtQuick
import Quickshell

ShellRoot {{
  QtObject {{
    id: testBar
    property bool vertical: {str(vertical).lower()}
    property int barSize: 26
    property color barForeground: "#dddddd"
    property color foreground: "#dddddd"
    property color urgent: "#ff7b72"
    property string fontFamily: "JetBrainsMono Nerd Font"
    property bool foregroundAnimationEnabled: true
    function registerClickTarget() {{}}
    function unregisterClickTarget() {{}}
    function showTooltip() {{}}
    function hideTooltip() {{}}
  }}

  Loader {{
    id: widgetLoader
    source: "{(PLUGIN / "BarWidget.qml").as_uri()}"
    onLoaded: {{
      item.bar = testBar
      console.warn("WIDGET_READY state=" + item.daemonState)
    }}
  }}

  Connections {{
    target: widgetLoader.item
    ignoreUnknownSignals: true
    function onDaemonStateChanged() {{
      console.warn("WIDGET_STATE state=" + widgetLoader.item.daemonState)
    }}
    function onDaemonUnavailableChanged() {{
      console.warn("WIDGET_UNAVAILABLE unavailable=" + widgetLoader.item.daemonUnavailable)
    }}
  }}
}}
'''
    )

    env = {
        **os.environ,
        "OMATALK_SOCKET": str(socket_path),
        "QML2_IMPORT_PATH": str(tmp_path / "imports"),
        "QT_QUICK_BACKEND": "software",
    }
    process = subprocess.Popen(
        [quickshell, "-p", str(tmp_path)],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )

    connection = None
    try:
        if not initially_available:
            time.sleep(3)
            server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            server.bind(str(socket_path))
            server.listen(1)
            server.settimeout(10)
        assert server is not None
        connection, _ = server.accept()
        connection.settimeout(10)
        assert connection.recv(64) == b"follow\n"
        connection.sendall(b"idle\n")
        if initially_available:
            output_until(process, "WIDGET_READY state=idle")

        connection.sendall(b"speaking\n")
        output_until(process, "WIDGET_STATE state=speaking")
        connection.sendall(b"idle\n")
        output_until(process, "WIDGET_STATE state=idle")

        connection.sendall(b"speaking\n")
        output_until(process, "WIDGET_STATE state=speaking")
        connection.sendall(b"unknown\n")
        output_until(process, "WIDGET_STATE state=idle")
        connection.sendall(b"error\n")
        output_until(process, "WIDGET_UNAVAILABLE unavailable=true")
        connection.sendall(b"idle\n")
        output_until(process, "WIDGET_UNAVAILABLE unavailable=false")

        connection.close()
        connection = None
        if server is not None:
            server.close()
        socket_path.unlink(missing_ok=True)
        output_until(process, "WIDGET_UNAVAILABLE unavailable=true", timeout=6)
        time.sleep(2)

        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        server.bind(str(socket_path))
        server.listen(1)
        server.settimeout(10)
        connection, _ = server.accept()
        connection.settimeout(10)
        assert connection.recv(64) == b"follow\n"
        output_until(process, "WIDGET_UNAVAILABLE unavailable=false")
        connection.sendall(b"speaking\n")
        output_until(process, "WIDGET_STATE state=speaking")
    finally:
        if connection is not None:
            connection.close()
        if server is not None:
            server.close()
        process.terminate()
        process.wait(timeout=10)


def ipc_call(pid: int, target: str, function: str, *args: str) -> str:
    result = subprocess.run(
        ["quickshell", "ipc", "--pid", str(pid), "call", target, function, *args],
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert result.returncode == 0, result.stderr
    return result.stdout.strip()


def wait_for_panel_state(pid: int, timeout: float = 10) -> dict:
    deadline = time.monotonic() + timeout
    state = json.loads(ipc_call(pid, "omatalkTestDriver", "state"))
    while time.monotonic() < deadline and not state["voiceOptions"]:
        time.sleep(0.1)
        state = json.loads(ipc_call(pid, "omatalkTestDriver", "state"))
    return state


def test_panel_shells_config_cli_for_voice_and_speed(tmp_path):
    quickshell = shutil.which("quickshell")
    shell_dir = Path("/usr/share/omarchy/shell")
    if not quickshell or not (shell_dir / "Ui/qmldir").is_file():
        pytest.skip("requires an installed Omarchy Quickshell shell")
    if not os.environ.get("WAYLAND_DISPLAY"):
        pytest.skip("requires a graphical Wayland session")

    imports = tmp_path / "imports" / "qs"
    imports.mkdir(parents=True)
    for module in ("Ui", "Commons"):
        (imports / module).symlink_to(shell_dir / module, target_is_directory=True)

    # Fake `omatalk` binary on PATH: logs every invocation and returns the
    # same canned JSON `config get`/`config voices` shell out to.
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    cli_log = tmp_path / "cli.log"
    (fake_bin / "omatalk").write_text(
        "#!/bin/sh\n"
        'printf "%s\\n" "$*" >> "$OMATALK_TEST_CLI_LOG"\n'
        'case "$*" in\n'
        f"  \"config voices --json\") echo '{json.dumps(FAKE_PANEL_VOICES)}' ;;\n"
        f"  \"config get --json\") echo '{json.dumps(FAKE_PANEL_CONFIG)}' ;;\n"
        "esac\n"
        "exit 0\n"
    )
    (fake_bin / "omatalk").chmod(0o755)

    shell = tmp_path / "shell.qml"
    shell.write_text(
        f'''import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {{
  QtObject {{
    id: testBar
    property bool vertical: false
    property int barSize: 26
    property color barForeground: "#dddddd"
    property color foreground: "#dddddd"
    property color background: "#1a1a1a"
    property color urgent: "#ff7b72"
    property string fontFamily: "JetBrainsMono Nerd Font"
    property bool foregroundAnimationEnabled: true
    property string position: "top"
    property var activePopout: null
    property var clickTargets: []
    function registerClickTarget() {{}}
    function unregisterClickTarget() {{}}
    function showTooltip() {{}}
    function hideTooltip() {{}}
    function requestPopout(key) {{ activePopout = key }}
    function releasePopout(key) {{ if (activePopout === key) activePopout = null }}
    function targetBelongsToWindow() {{ return true }}
  }}

  function findByObjectName(item, name) {{
    if (!item) return null
    if (item.objectName === name) return item
    if (!item.children) return null
    for (var i = 0; i < item.children.length; i++) {{
      var found = findByObjectName(item.children[i], name)
      if (found) return found
    }}
    return null
  }}

  Loader {{
    id: widgetLoader
    source: "{(PLUGIN / "BarWidget.qml").as_uri()}"
    onLoaded: {{
      item.bar = testBar
      console.warn("WIDGET_READY state=" + item.daemonState)
    }}
  }}

  IpcHandler {{
    target: "omatalkTestDriver"
    function openPanel(): void {{ widgetLoader.item.togglePanel() }}
    function state(): string {{
      var p = widgetLoader.item ? widgetLoader.item.panelItem : null
      if (!p) return "{{}}"
      return JSON.stringify({{
        voiceOptions: p.voiceOptions,
        voice: p.voice,
        speed: p.speed,
        opened: p.opened
      }})
    }}
    function setVoice(v: string): void {{
      var d = findByObjectName(widgetLoader.item.panelItem.contentRoot, "omatalkVoiceDropdown")
      if (d) d.changed(v)
    }}
    function dragAndReleaseSpeed(v: string): void {{
      var s = findByObjectName(widgetLoader.item.panelItem.contentRoot, "omatalkSpeedSlider")
      if (!s) return
      s.moved(0.6)
      s.moved(0.8)
      s.released(parseFloat(v))
    }}
  }}
}}
'''
    )

    env = {
        **os.environ,
        "PATH": f"{fake_bin}:{os.environ['PATH']}",
        "OMATALK_TEST_CLI_LOG": str(cli_log),
        "OMATALK_SOCKET": str(tmp_path / "missing.sock"),
        "QML2_IMPORT_PATH": str(tmp_path / "imports"),
        "QT_QUICK_BACKEND": "software",
    }
    process = subprocess.Popen(
        [quickshell, "-p", str(tmp_path)],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    try:
        output_until(process, "WIDGET_READY state=idle")

        ipc_call(process.pid, "omatalkTestDriver", "openPanel")
        state = wait_for_panel_state(process.pid)

        # Opening the panel populates the dropdown/slider from the CLI's
        # canned config, and shells voices/get exactly once each.
        assert state["opened"] is True
        assert state["voiceOptions"] == FAKE_PANEL_VOICES
        assert state["voice"] == "af_test_one"
        assert state["speed"] == 1.25
        log_lines = cli_log.read_text().splitlines()
        assert log_lines.count("config voices --json") == 1
        assert log_lines.count("config get --json") == 1

        ipc_call(process.pid, "omatalkTestDriver", "setVoice", "bf_test_two")
        deadline = time.monotonic() + 10
        while (
            time.monotonic() < deadline
            and "config set voice bf_test_two" not in cli_log.read_text()
        ):
            time.sleep(0.1)
        log_lines = cli_log.read_text().splitlines()
        assert log_lines.count("config set voice bf_test_two") == 1

        # 1.73 is deliberately not a clean tenth, to prove the panel snaps
        # a drag's continuous release value to the nearest 0.1 itself
        # (PanelSlider's own `step` only affects wheel nudges, not drags).
        ipc_call(process.pid, "omatalkTestDriver", "dragAndReleaseSpeed", "1.73")
        deadline = time.monotonic() + 10
        while (
            time.monotonic() < deadline
            and "config set speed 1.7" not in cli_log.read_text()
        ):
            time.sleep(0.1)
        log_lines = cli_log.read_text().splitlines()
        # Dragging must not have shelled out per-tick — only the release does.
        assert log_lines.count("config set speed 1.7") == 1
        assert not any(line.startswith("config set speed 0.6") for line in log_lines)
        assert not any(line.startswith("config set speed 0.8") for line in log_lines)
        assert not any(line.startswith("config set speed 1.73") for line in log_lines)
    finally:
        process.terminate()
        process.wait(timeout=10)
