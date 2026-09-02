#!/usr/bin/env bash
# Thin, stable dispatcher: resolves which uninstall.sh to actually run — the
# one that shipped with the latest release by default, or a specific branch
# via OMATALK_REF for testing — and execs it. This file should rarely need
# to change; the real uninstaller lives at the repo root and evolves with
# the project, one release at a time.
set -euo pipefail

REPO="zerobearing2/omatalk"
if [ -n "${OMATALK_REF:-}" ]; then
  url="https://raw.githubusercontent.com/$REPO/$OMATALK_REF/uninstall.sh"
else
  url="https://github.com/$REPO/releases/latest/download/uninstall.sh"
fi

# bash -c "$(curl ...)" rather than curl | bash: leaves stdin free for the
# fetched script's own interactive prompts instead of consuming it as source.
exec bash -c "$(curl -fsSL "$url")"
