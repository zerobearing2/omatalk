import os
import tomllib
from pathlib import Path

DEFAULTS = {
    "voice": "af_heart",
    "speed": 1.0,
    "lang": "en-us",
    "capture_primary": ["wl-paste", "--primary"],
    "capture_clipboard": ["wl-paste"],
    "player": ["pw-play"],
    "notify": ["notify-send", "Omatalk"],
    "idle_timeout": 600,
}


def load() -> dict:
    cfg = dict(DEFAULTS)
    path = Path(
        os.environ.get("OMATALK_CONFIG", "~/.config/omatalk/config.toml")
    ).expanduser()
    if path.exists():
        cfg.update(tomllib.loads(path.read_text()))
    return cfg


def socket_path() -> Path:
    return Path(
        os.environ.get(
            "OMATALK_SOCKET",
            Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
            / "omatalk"
            / "omatalk.sock",
        )
    )


def models_path() -> Path:
    return Path(
        os.environ.get("OMATALK_MODELS", "~/.local/share/omatalk/models")
    ).expanduser()
