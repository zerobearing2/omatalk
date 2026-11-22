#!/usr/bin/env bash
set -euo pipefail

OMATALK_BIN="${OMATALK_BIN:-/usr/bin/omatalk}"

msg() {
  printf '\033[1;32m==>\033[0m %s\n' "$1"
}

if ! command -v omarchy >/dev/null 2>&1; then
  msg "Omatalk requires Omarchy. Install Omarchy, then rerun."
  exit 1
fi

msg "Installing the omatalk-bin package"
if ! omarchy pkg add omatalk-bin; then
  msg "Could not install omatalk-bin; nothing was changed"
  exit 1
fi

exec "$OMATALK_BIN" setup
