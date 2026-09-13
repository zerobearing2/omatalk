#!/usr/bin/env bash
# Tests plus the committed pin matching this tree's pack.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

uv run --group dev pytest tests/
scripts/verify-pin.sh
