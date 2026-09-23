import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXPORT = [
    "uv",
    "export",
    "--locked",
    "--no-dev",
    "--group",
    "build",
    "--no-emit-project",
    "--no-header",
    "--format",
    "requirements-txt",
]


def test_requirements_txt_matches_uv_lock():
    result = subprocess.run(
        EXPORT, cwd=ROOT, capture_output=True, text=True, check=True
    )
    committed = (ROOT / "requirements.txt").read_text()
    assert committed == result.stdout, (
        "requirements.txt is stale; run: " + " ".join(EXPORT) + " -o requirements.txt"
    )
