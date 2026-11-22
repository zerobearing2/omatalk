#!/usr/bin/env bash
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
OMATALK_BIN="${OMATALK_BIN:-/usr/bin/omatalk}"
UNIT="$HOME/.config/systemd/user/omatalk.service"

msg() {
  printf '\033[1;32m==>\033[0m %s\n' "$1"
}

remove_curl_era_files() {
  local stop_status

  if [ -e "$UNIT" ]; then
    set +e
    systemctl --user disable --now omatalk.service 2>/dev/null
    stop_status=$?
    set -e
    if [ "$stop_status" -ne 0 ]; then
      if [ "$stop_status" -ne 5 ]; then
        msg "Could not stop the current daemon; refusing to remove its files"
        exit "$stop_status"
      fi
    fi
    msg "Removing the curl-era systemd unit"
    rm -f "$UNIT"
    systemctl --user daemon-reload
  fi

  rm -f "$HOME/.local/bin/omatalk" "$HOME/.local/bin/omatalkd"
  rm -rf "$OMATALK_HOME/venv" "$OMATALK_HOME/src"
  rm -f "$OMATALK_HOME/models/kokoro-v1.0.fp16.onnx" "$OMATALK_HOME/omatalk-src.tar.gz" \
    "$OMATALK_HOME/omatalk-src.tar.gz.sha256"
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

remove_curl_era_files

exec "$OMATALK_BIN" setup
