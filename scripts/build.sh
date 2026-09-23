#!/usr/bin/env bash
# Pack the runtime tarball and write RELEASE_TAG + TARBALL_SHA256 into
# install.sh. Tag is v$(pyproject version).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

pack_only=0
if [ "${1:-}" = "--pack-only" ]; then
  pack_only=1
fi

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' pyproject.toml)"
if [ -z "$version" ]; then
  echo "could not read version from pyproject.toml" >&2
  exit 1
fi
tag="v$version"

# Reproducible: bytes depend only on the listed paths. install.sh is not
# in this archive; it is a sibling GitHub release asset.
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
  requirements.txt \
  uninstall.sh \
  | gzip -n > omatalk-src.tar.gz

sha256sum omatalk-src.tar.gz > omatalk-src.tar.gz.sha256
digest="$(sha256sum omatalk-src.tar.gz)"
digest="${digest%% *}"

if [ "$pack_only" -eq 1 ]; then
  printf 'packed %s %s\n' "$tag" "$digest"
  exit 0
fi

rewritten="$(mktemp)"
trap 'rm -f "$rewritten"' EXIT

wrote_tag=0
wrote_digest=0
while IFS= read -r line; do
  case "$line" in
    RELEASE_TAG=*)
      printf 'RELEASE_TAG="%s"\n' "$tag"
      wrote_tag=1
      ;;
    TARBALL_SHA256=*)
      printf 'TARBALL_SHA256="%s"\n' "$digest"
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
printf 'built %s %s\n' "$tag" "$digest"
