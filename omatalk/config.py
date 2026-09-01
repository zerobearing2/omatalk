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
}


def config_path() -> Path:
    return Path(
        os.environ.get("OMATALK_CONFIG", "~/.config/omatalk/config.toml")
    ).expanduser()


def load() -> dict:
    cfg = dict(DEFAULTS)
    path = config_path()
    if path.exists():
        cfg.update(tomllib.loads(path.read_text()))
    return cfg


def _toml_literal(value) -> str:
    if isinstance(value, str):
        return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'
    if isinstance(value, float):
        return repr(value)
    if isinstance(value, int):
        return str(value)
    if isinstance(value, list):
        return "[" + ", ".join(_toml_literal(v) for v in value) + "]"
    raise TypeError(f"unsupported config value type: {type(value)!r}")


def set(key: str, value) -> None:
    """Write one key to config.toml, leaving every other key (including ones
    the user hand-set and this module has never touched) untouched. Only the
    raw file overrides are read here, never DEFAULTS-merged, so a key left at
    its default is never written and stays free to pick up future default
    changes."""
    path = config_path()
    overrides = tomllib.loads(path.read_text()) if path.exists() else {}
    overrides[key] = value
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"{k} = {_toml_literal(v)}" for k, v in overrides.items()]
    path.write_text("\n".join(lines) + "\n")


def voices() -> list:
    """Voice names, read straight from the voices archive — no Kokoro/
    onnxruntime session required, so this works without the Daemon."""
    import numpy

    with numpy.load(models_path() / "voices-v1.0.bin") as archive:
        return sorted(archive.files)


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
