#!/usr/bin/env bash
# Pack this tree and write RELEASE_TAG + TARBALL_SHA256 into install.sh.
# Tag is v$(pyproject version). Commit install.sh, push, then make release.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' pyproject.toml)"
if [ -z "$version" ]; then
  echo "could not read version from pyproject.toml" >&2
  exit 1
fi
tag="v$version"

tmp="$(mktemp)"
rewritten="$(mktemp)"
trap 'rm -f "$tmp" "$rewritten"' EXIT
scripts/pack-src.sh "$tmp"
digest="$(sha256sum "$tmp")"
digest="${digest%% *}"

wrote_tag=0
wrote_digest=0
while IFS= read -r line; do
  case "$line" in
    RELEASE_TAG=*)
      printf 'RELEASE_TAG="${RELEASE_TAG:-%s}"\n' "$tag"
      wrote_tag=1
      ;;
    TARBALL_SHA256=*)
      printf 'TARBALL_SHA256="${TARBALL_SHA256:-%s}"\n' "$digest"
      wrote_digest=1
      ;;
    *)
      printf '%s\n' "$line"
      ;;
  esac
done < install.sh > "$rewritten"

if [ "$wrote_tag" -ne 1 ]; then
  echo "install.sh has no RELEASE_TAG= line to rewrite" >&2
  exit 1
fi
if [ "$wrote_digest" -ne 1 ]; then
  echo "install.sh has no TARBALL_SHA256= line to rewrite" >&2
  exit 1
fi

mv "$rewritten" install.sh
chmod +x install.sh
printf 'pinned %s %s\n' "$tag" "$digest"
