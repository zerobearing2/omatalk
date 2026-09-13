#!/usr/bin/env bash
# Print "tag digest" as committed in install.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
install_sh="$ROOT/install.sh"

tag=""
digest=""
while IFS= read -r line; do
  case "$line" in
    RELEASE_TAG=*)
      tag="${line#*:-}"
      tag="${tag%\"}"
      tag="${tag%\}}"
      ;;
    TARBALL_SHA256=*)
      digest="${line#*:-}"
      digest="${digest%\"}"
      digest="${digest%\}}"
      ;;
  esac
done < "$install_sh"

if [ -z "$tag" ]; then
  echo "could not read RELEASE_TAG from install.sh" >&2
  exit 1
fi
if [ -z "$digest" ]; then
  echo "could not read TARBALL_SHA256 from install.sh" >&2
  exit 1
fi

printf '%s %s\n' "$tag" "$digest"
