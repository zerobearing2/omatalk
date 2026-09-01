import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from .config import socket_path

SITE_BASE = "https://omatalk.zerobearing.com"
USAGE = "usage: omatalk speak|stop|status|upgrade"


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
        result = subprocess.run(["bash", path], check=False)
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"upgrade failed: {error}", file=sys.stderr)
        sys.exit(1)
    finally:
        if path:
            Path(path).unlink(missing_ok=True)
    sys.exit(result.returncode)


def main():
    args = sys.argv[1:]
    if args and args[0] == "upgrade":
        if len(args) != 1:
            print(USAGE, file=sys.stderr)
            sys.exit(2)
        upgrade()
    if (not args or args[0] not in ("speak", "stop", "status")
            or (args[0] != "speak" and len(args) > 1)):
        print(USAGE, file=sys.stderr)
        sys.exit(2)
    cmd = " ".join(args)
    try:
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        client.settimeout(5)
        client.connect(str(socket_path()))
        client.sendall((cmd + "\n").encode())
        print(client.recv(1024).decode().strip())
    except OSError:
        subprocess.run(
            [
                "notify-send",
                "Omatalk",
                "daemon not running — systemctl --user start omatalk",
            ]
        )
        print("daemon not running", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
