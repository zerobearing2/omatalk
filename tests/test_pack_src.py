import hashlib
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def _committed_pin():
    tag = None
    digest = None
    for line in (ROOT / "install.sh").read_text().splitlines():
        if line.startswith("RELEASE_TAG="):
            tag = line.split(":-", 1)[1].rstrip('}"')
        elif line.startswith("TARBALL_SHA256="):
            digest = line.split(":-", 1)[1].rstrip('}"')
    assert tag, "install.sh has no RELEASE_TAG="
    assert digest, "install.sh has no TARBALL_SHA256="
    return tag, digest


def _pyproject_version():
    text = (ROOT / "pyproject.toml").read_text()
    match = re.search(r'^version = "([^"]+)"$', text, re.M)
    assert match, "could not read version from pyproject.toml"
    return match.group(1)


def _pack():
    result = subprocess.run(
        ["bash", str(ROOT / "scripts/build.sh"), "--pack-only"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    digest = hashlib.sha256((ROOT / "omatalk-src.tar.gz").read_bytes()).hexdigest()
    assert digest in result.stdout
    return digest


def test_pack_is_byte_identical_across_runs_and_leaves_install_sh():
    before = (ROOT / "install.sh").read_text()
    first = _pack()
    second = _pack()
    assert first == second
    assert len(first) == 64
    assert (ROOT / "install.sh").read_text() == before


def test_committed_pin_matches_this_tree():
    before = (ROOT / "install.sh").read_text()
    tag, expected = _committed_pin()
    assert tag == f"v{_pyproject_version()}"
    digest = _pack()
    assert digest == expected
    assert (ROOT / "install.sh").read_text() == before
