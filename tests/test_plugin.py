import json
import os
import shutil
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
PLUGIN = ROOT / "plugin"


def test_manifest_points_to_bar_widget():
    manifest = json.loads((PLUGIN / "manifest.json").read_text())

    assert manifest["kinds"] == ["bar-widget"]
    assert manifest["entryPoints"]["barWidget"] == "BarWidget.qml"
    assert (PLUGIN / manifest["entryPoints"]["barWidget"]).is_file()
    assert (PLUGIN / "Panel.qml").is_file()
    assert manifest["id"] == "zerobearing.omatalk"
    assert manifest["description"] == (
        "Local text-to-speech: select text, press F8, the machine reads it back."
    )


def test_plugin_tree_is_marketplace_ready():
    names = {path.name for path in PLUGIN.iterdir()}
    assert {"README.md", "LICENSE", "preview.png", "manifest.json"} <= names
    assert "install.sh" not in names
    assert "uninstall.sh" not in names
    assert not any(path.suffix == ".py" for path in PLUGIN.iterdir())
    assert not any(path.is_symlink() for path in PLUGIN.iterdir())
    readme = (PLUGIN / "README.md").read_text()
    assert "omarchy plugin add https://github.com/zerobearing2/omarchy-omatalk-plugin.git --enable" in readme
    assert "omarchy plugin remove zerobearing.omatalk" in readme
    assert "uninstall.sh" in readme
    assert "No sudo" in readme

    validator = shutil.which("omarchy")
    if not validator:
        pytest.skip("omarchy CLI not installed")
    result = subprocess.run(
        ["omarchy", "plugin", "validate", str(PLUGIN)],
        capture_output=True,
        text=True,
        timeout=15,
    )
    assert result.returncode == 0, result.stdout + result.stderr


def qmltestrunner():
    candidates = [
        Path("/usr/lib/qt6/bin/qmltestrunner"),
        shutil.which("qmltestrunner"),
    ]
    for path in candidates:
        if path and Path(path).is_file():
            return str(path)
    return None


def test_panel_qml():
    runner = qmltestrunner()
    if not runner:
        pytest.skip("qmltestrunner not installed")

    result = subprocess.run(
        [
            runner,
            "-input",
            str(ROOT / "tests" / "qml"),
            "-import",
            str(ROOT / "tests" / "qml" / "imports"),
            "-o",
            "-,txt",
        ],
        env={
            **os.environ,
            "QT_QPA_PLATFORM": "offscreen",
            "QT_QUICK_BACKEND": "software",
        },
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode == 0, result.stdout + result.stderr
