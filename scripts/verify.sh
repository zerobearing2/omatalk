#!/usr/bin/env bash
# Tests plus install.sh naming this tree's version and tarball digest.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

uv run --group dev pytest tests/

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' pyproject.toml)"
if [ -z "$version" ]; then
  echo "could not read version from pyproject.toml" >&2
  exit 1
fi

tag=""
expected=""
while IFS= read -r line; do
  case "$line" in
    RELEASE_TAG=*)
      tag="${line#*:-}"
      tag="${tag%\"}"
      tag="${tag%\}}"
      ;;
    TARBALL_SHA256=*)
      expected="${line#*:-}"
      expected="${expected%\"}"
      expected="${expected%\}}"
      ;;
  esac
done < install.sh

if [ -z "$tag" ]; then
  echo "could not read RELEASE_TAG from install.sh" >&2
  exit 1
fi
if [ -z "$expected" ]; then
  echo "could not read TARBALL_SHA256 from install.sh" >&2
  exit 1
fi

if [ "$tag" != "v$version" ]; then
  echo "install.sh RELEASE_TAG is $tag but pyproject.toml is $version (run scripts/build.sh)" >&2
  exit 1
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
tar --sort=name \
  --mtime=@0 \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  --mode=u=rwX,go=rX \
  --exclude=__pycache__ \
  --exclude='*.pyc' \
  --exclude='*.pyo' \
  --transform=s,^,omatalk/, \
  -cf - \
  daemon \
  systemd \
  pyproject.toml \
  README.md \
  uninstall.sh \
  | gzip -n > "$tmp"
actual="$(sha256sum "$tmp")"
actual="${actual%% *}"

if [ "$actual" != "$expected" ]; then
  echo "install.sh TARBALL_SHA256 is $expected but this tree packs $actual (run scripts/build.sh)" >&2
  exit 1
fi

printf 'verified %s %s\n' "$tag" "$expected"
