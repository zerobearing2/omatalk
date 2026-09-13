import hashlib
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def test_build_is_byte_identical_across_runs(tmp_path):
    subprocess.run(["bash", str(ROOT / "scripts/build.sh")], cwd=ROOT, check=True)
    first = (ROOT / "omatalk-src.tar.gz").read_bytes()
    subprocess.run(["bash", str(ROOT / "scripts/build.sh")], cwd=ROOT, check=True)
    second = (ROOT / "omatalk-src.tar.gz").read_bytes()
    assert first == second
    assert len(hashlib.sha256(first).hexdigest()) == 64


def test_committed_pin_matches_this_tree():
    result = subprocess.run(
        ["bash", str(ROOT / "scripts/build.sh")],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    digest = hashlib.sha256((ROOT / "omatalk-src.tar.gz").read_bytes()).hexdigest()
    assert digest in result.stdout
    install = (ROOT / "install.sh").read_text()
    assert f'TARBALL_SHA256="${{TARBALL_SHA256:-{digest}}}"' in install
