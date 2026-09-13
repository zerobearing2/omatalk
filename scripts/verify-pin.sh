#!/usr/bin/env bash
# Fail unless install.sh names this tree's version and the tarball this
# tree packs. Same check locally and in the release workflow.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' pyproject.toml)"
if [ -z "$version" ]; then
  echo "could not read version from pyproject.toml" >&2
  exit 1
fi

pin="$(scripts/read-pin.sh)"
tag="${pin%% *}"
expected="${pin##* }"

if [ "$tag" != "v$version" ]; then
  echo "install.sh RELEASE_TAG is $tag but pyproject.toml is $version (run make pin)" >&2
  exit 1
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
scripts/pack-src.sh "$tmp"
actual="$(sha256sum "$tmp")"
actual="${actual%% *}"

if [ "$actual" != "$expected" ]; then
  echo "install.sh TARBALL_SHA256 is $expected but this tree packs $actual (run make pin)" >&2
  exit 1
fi

printf 'pin ok: %s %s\n' "$tag" "$expected"
