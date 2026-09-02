import argparse
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from . import config

SITE_BASE = "https://omatalk.zerobearing.com"

# The only keys `config set` will touch in this phase. Anything else —
# unknown, or one of the existing list-valued keys — is rejected rather than
# silently accepted, so a future phase can widen this without a protocol
# change.
CONFIG_SETTABLE = {"voice", "speed"}


def upgrade():
    site = os.environ.get("SITE_BASE", SITE_BASE).rstrip("/")
    path = None
    try:
        with tempfile.NamedTemporaryFile(
            prefix="omatalk-upgrade-", suffix=".sh", delete=False
        ) as script:
            path = script.name
        subprocess.run(
            [
                "curl",
                "-L",
                "--fail",
                "--silent",
                "--show-error",
                "-o",
                path,
                f"{site}/install.sh?ts={int(time.time())}",
            ],
            check=True,
        )
        os.execvpe(
            "bash",
            [
                "bash",
                "-c",
                'trap \'status=$?; rm -f -- "$1"; exit "$status"\' EXIT; bash "$1"',
                "omatalk upgrade",
                path,
            ],
            os.environ,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"upgrade failed: {error}", file=sys.stderr)
        sys.exit(1)
    finally:
        if path:
            Path(path).unlink(missing_ok=True)


def notify_daemon_down():
    try:
        subprocess.run(
            [
                "notify-send",
                "Omatalk",
                "daemon not running — systemctl --user start omatalk",
            ]
        )
    except OSError:
        pass


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="omatalk")
    sub = parser.add_subparsers(dest="command", required=True)

    speak = sub.add_parser("speak", help="speak text, or the current selection/clipboard")
    speak.add_argument("text", nargs="*")
    speak.add_argument(
        "--voice", help="speak this one Utterance in a voice, without changing the default"
    )

    sub.add_parser("stop", help="stop speaking")
    sub.add_parser("status", help="print the daemon's state")
    sub.add_parser("upgrade", help="install the latest release")

    config_parser = sub.add_parser("config", help="get or set voice/speed")
    config_sub = config_parser.add_subparsers(dest="config_command", required=True)

    get = config_sub.add_parser("get", help="print the effective configuration")
    get.add_argument("--json", action="store_true")

    set_ = config_sub.add_parser("set", help="set one config key")
    set_.add_argument("key")
    set_.add_argument("value")

    voices = config_sub.add_parser("voices", help="list available voices")
    voices.add_argument("--json", action="store_true")

    return parser


def _print_json_or_lines(as_json: bool, data, plain_lines) -> None:
    if as_json:
        print(json.dumps(data))
    else:
        for line in plain_lines:
            print(line)


def config_get(as_json: bool) -> int:
    cfg = config.load()
    _print_json_or_lines(as_json, cfg, (f"{key} = {cfg[key]}" for key in sorted(cfg)))
    return 0


def config_voices(as_json: bool) -> int:
    names = config.voices()
    _print_json_or_lines(as_json, names, names)
    return 0


def known_voice(name: str) -> bool:
    # Single source of truth for voice validity: `config set voice` and
    # `speak --voice` both call this instead of keeping two lists that could
    # drift apart.
    if name in config.voices():
        return True
    print(f"{name}: not a known voice", file=sys.stderr)
    return False


def config_set(key: str, raw_value: str) -> int:
    if key not in CONFIG_SETTABLE:
        print(
            f"{key}: not settable via config set (edit config.toml directly)",
            file=sys.stderr,
        )
        return 1

    if key == "voice":
        if not known_voice(raw_value):
            return 1
        config.set("voice", raw_value)
        return 0

    # key == "speed"
    try:
        value = float(raw_value)
    except ValueError:
        print(f"{raw_value}: speed must be a number", file=sys.stderr)
        return 1
    if not (0.5 <= value <= 2.0):
        print(f"{raw_value}: speed must be between 0.5 and 2.0", file=sys.stderr)
        return 1
    config.set("speed", value)
    return 0


def send_daemon_command(cmd: str, command: str) -> int:
    try:
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        client.settimeout(5)
        client.connect(str(config.socket_path()))
        client.sendall((cmd + "\n").encode())
        print(client.recv(1024).decode().strip())
        return 0
    except OSError:
        if command != "status":
            notify_daemon_down()
        print("daemon not running", file=sys.stderr)
        return 1


def main():
    parser = build_parser()
    args = parser.parse_args()

    if args.command == "upgrade":
        upgrade()
        return

    if args.command == "config":
        if args.config_command == "get":
            sys.exit(config_get(args.json))
        if args.config_command == "set":
            sys.exit(config_set(args.key, args.value))
        if args.config_command == "voices":
            sys.exit(config_voices(args.json))
        return

    if args.command == "speak":
        text = " ".join(args.text)
        if args.voice:
            if not known_voice(args.voice):
                sys.exit(1)
            cmd = f"speak --voice {args.voice} {text}" if text else f"speak --voice {args.voice}"
        else:
            cmd = f"speak {text}" if text else "speak"
    else:
        cmd = args.command

    sys.exit(send_daemon_command(cmd, args.command))


if __name__ == "__main__":
    main()
