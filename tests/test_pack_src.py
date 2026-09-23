import hashlib
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


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
