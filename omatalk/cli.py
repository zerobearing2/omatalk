import socket
import subprocess
import sys

from .config import socket_path

USAGE = "usage: omatalk speak|stop|status"


def main():
    args = sys.argv[1:]
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
