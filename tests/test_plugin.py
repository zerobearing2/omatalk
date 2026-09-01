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


def test_manifest_points_to_megaphone():
    manifest = json.loads((PLUGIN / "manifest.json").read_text())

    assert manifest["kinds"] == ["bar-widget"]
    assert manifest["entryPoints"]["barWidget"] == "Megaphone.qml"
    assert (PLUGIN / manifest["entryPoints"]["barWidget"]).is_file()


@pytest.mark.parametrize("vertical", [False, True])
def test_megaphone_socket_widget_round_trip(tmp_path, vertical):
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
    source: "{(PLUGIN / "Megaphone.qml").as_uri()}"
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
        connection, _ = server.accept()
        connection.settimeout(10)
        assert connection.recv(64) == b"follow\n"
        connection.sendall(b"idle\n")
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
        server.close()
        socket_path.unlink(missing_ok=True)
        output_until(process, "WIDGET_UNAVAILABLE unavailable=true", timeout=6)

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
        server.close()
        process.terminate()
        process.wait(timeout=10)
