#!/usr/bin/env bash
# Thin, stable dispatcher: fetches the install.sh that shipped with the
# latest release and execs it. This file should rarely need to change;
# the real installer lives at the repo root and evolves with the project,
# one release at a time.
set -euo pipefail

REPO="zerobearing2/omatalk"
url="https://github.com/$REPO/releases/latest/download/install.sh"

# bash -c "$(curl ...)" rather than curl | bash: leaves stdin free for the
# fetched script's own interactive prompts instead of consuming it as source.
exec bash -c "$(curl -fsSL "$url")"
