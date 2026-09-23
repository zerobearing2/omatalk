#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
plugin="$ROOT/plugin"

if [ ! -e "$plugin/.git" ]; then
  echo "git submodule update --init" >&2
  exit 1
fi

"$plugin/tests/run.sh"
