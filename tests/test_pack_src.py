import hashlib
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def pack_to(path):
    subprocess.run(
        ["bash", str(ROOT / "scripts/pack-src.sh"), str(path)],
        cwd=ROOT,
        check=True,
    )


def test_pack_is_byte_identical_across_runs(tmp_path):
    first = tmp_path / "a.tar.gz"
    second = tmp_path / "b.tar.gz"
    pack_to(first)
    pack_to(second)
    assert first.read_bytes() == second.read_bytes()
    digest = hashlib.sha256(first.read_bytes()).hexdigest()
    assert len(digest) == 64


def test_committed_pin_matches_this_tree():
    env = {**os.environ}
    result = subprocess.run(
        ["bash", str(ROOT / "scripts/verify-pin.sh")],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr + result.stdout
    assert result.stdout.startswith("pin ok:")
