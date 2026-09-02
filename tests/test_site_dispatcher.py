import os
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent


@pytest.fixture
def fake_curl(tmp_path):
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    curl = bin_dir / "curl"
    curl.write_text(
        """#!/bin/sh
# The URL is the last argument. Emit a tiny script that echoes it back,
# standing in for whatever install.sh/uninstall.sh would have been.
for arg in "$@"; do url="$arg"; done
echo "echo FETCHED_URL=$url"
"""
    )
    curl.chmod(0o755)
    return bin_dir


def run_dispatcher(script, fake_curl, ref=None):
    env = {**os.environ, "PATH": f"{fake_curl}:{os.environ['PATH']}"}
    if ref is None:
        env.pop("OMATALK_REF", None)
    else:
        env["OMATALK_REF"] = ref
    return subprocess.run(
        ["bash", str(ROOT / "public" / script)],
        env=env,
        capture_output=True,
        text=True,
    )


@pytest.mark.parametrize("script", ["install.sh", "uninstall.sh"])
def test_dispatcher_fetches_the_latest_release_by_default(script, fake_curl):
    result = run_dispatcher(script, fake_curl)

    assert result.returncode == 0, result.stderr
    assert (
        f"FETCHED_URL=https://github.com/zerobearing2/omatalk/releases/latest/download/{script}"
        in result.stdout
    )


@pytest.mark.parametrize("script", ["install.sh", "uninstall.sh"])
def test_dispatcher_fetches_the_ref_branch_when_set(script, fake_curl):
    result = run_dispatcher(script, fake_curl, ref="config-screen")

    assert result.returncode == 0, result.stderr
    assert (
        f"FETCHED_URL=https://raw.githubusercontent.com/zerobearing2/omatalk/config-screen/{script}"
        in result.stdout
    )
