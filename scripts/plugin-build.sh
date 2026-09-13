#!/usr/bin/env bash
# Copy root install.sh into the plugin checkout.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
plugin="$ROOT/plugin"

if [ ! -e "$plugin/.git" ]; then
  echo "git submodule update --init" >&2
  exit 1
fi
if [ "$(git -C "$plugin" rev-parse --show-toplevel)" != "$plugin" ]; then
  echo "plugin/ is not the submodule" >&2
  exit 1
fi

cp -f "$ROOT/install.sh" "$plugin/install.sh"
chmod +x "$plugin/install.sh"
echo "copied install.sh to plugin/"
