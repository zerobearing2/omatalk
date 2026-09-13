#!/usr/bin/env bash
# Local Daemon release: test, pin this tree, commit install.sh if needed,
# push master, create the GitHub release from the tarball we just packed.
# Does not clobber an existing tag. Does not vendor the plugin.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ "$(git rev-parse --abbrev-ref HEAD)" != master ]; then
  echo "need master" >&2
  exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "working tree must be clean" >&2
  exit 1
fi

git fetch origin master --tags

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' pyproject.toml)"
if [ -z "$version" ]; then
  echo "could not read version from pyproject.toml" >&2
  exit 1
fi
tag="v$version"

remote_tag="$(git ls-remote --tags origin "refs/tags/$tag")"
if [ -n "$remote_tag" ]; then
  echo "$tag already exists — bump pyproject.toml first (make bump)" >&2
  exit 1
fi

if [ "$(git merge-base HEAD origin/master)" != "$(git rev-parse origin/master)" ]; then
  echo "master is behind origin; pull first" >&2
  exit 1
fi

uv run --group dev pytest tests/

scripts/pin-install.sh
scripts/verify-pin.sh

git add install.sh
if git diff --cached --quiet -- install.sh; then
  echo "install.sh already pinned to $tag"
else
  git commit -m "Pin install.sh to $tag"
fi

git push origin master

if [ ! -f omatalk-src.tar.gz ]; then
  echo "missing omatalk-src.tar.gz after pin" >&2
  exit 1
fi
if [ ! -f omatalk-src.tar.gz.sha256 ]; then
  echo "missing omatalk-src.tar.gz.sha256 after pin" >&2
  exit 1
fi

# The tarball pin packed is the asset. install.sh/uninstall.sh go up as
# siblings so the site dispatcher fetches a script already pinned to it.
gh release create "$tag" --title "$tag" --generate-notes \
  --target "$(git rev-parse HEAD)" \
  omatalk-src.tar.gz omatalk-src.tar.gz.sha256 install.sh uninstall.sh

echo "Released $tag"
