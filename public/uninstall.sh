#!/usr/bin/env bash
# Old docs pipe this file into bash. A package install is removed with the
# package. A curl install gets the last curl-era uninstaller, from v0.11.0:
# later releases ship no uninstall.sh.
set -euo pipefail

if omarchy pkg present omatalk-bin >/dev/null 2>&1; then
  echo "Run: omarchy pkg drop omatalk-bin" >&2
  exit 1
fi

url="https://github.com/zerobearing2/omatalk/releases/download/v0.11.0/uninstall.sh"

# bash -c "$(curl ...)" rather than curl | bash: leaves stdin free for the
# fetched script's own interactive prompts instead of consuming it as source.
exec bash -c "$(curl -fsSL "$url")"
