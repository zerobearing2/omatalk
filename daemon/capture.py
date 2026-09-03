import subprocess


def _run(cfg: dict, key: str) -> str:
    try:
        result = subprocess.run(cfg[key], capture_output=True, text=True, timeout=2)
    except subprocess.SubprocessError:
        return ""
    if result.returncode == 0:
        return result.stdout.strip()
    return ""


def capture_primary(cfg: dict) -> str:
    return _run(cfg, "capture_primary")


def capture_clipboard(cfg: dict) -> str:
    return _run(cfg, "capture_clipboard")
