#!/usr/bin/env bash
# Dogfood this checkout: build the release binary, replace ~/.local/bin/omatalk
# with it, install this tree's systemd unit, and restart the Daemon. Models
# come from a prior install.sh run; this script downloads nothing.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source scripts/lib.sh

LAUNCHER="$HOME/.local/bin/omatalk"
MODELS="$HOME/.local/share/omatalk/models"

for pin in MODEL_FILE VOICES_FILE; do
  file="$(installer_pin install.sh "$pin")"
  if [ ! -f "$MODELS/$file" ]; then
    echo "error: $MODELS/$file is missing; nothing was changed" >&2
    exit 1
  fi
done

cargo build --release --locked

mkdir -p "$HOME/.local/bin" "$HOME/.config/systemd/user"
launcher_tmp="$(mktemp "$LAUNCHER.XXXXXX")"
trap 'rm -f "$launcher_tmp"' EXIT
install -m 0755 "${CARGO_TARGET_DIR:-$ROOT/target}/release/omatalk" "$launcher_tmp"
mv -f "$launcher_tmp" "$LAUNCHER"
install -m 0644 systemd/omatalk.service "$HOME/.config/systemd/user/omatalk.service"

systemctl --user daemon-reload
systemctl --user reenable omatalk.service
systemctl --user restart omatalk.service
printf 'Dev install active: %s is %s\n' "$LAUNCHER" "$("$LAUNCHER" version)"
