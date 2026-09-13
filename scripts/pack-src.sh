#!/usr/bin/env bash
# Reproducible runtime tarball. Bytes depend only on the listed paths, not
# on who packs, when, or local file modes. install.sh is not in this
# archive; the installer is a sibling GitHub release asset.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

out="${1:-omatalk-src.tar.gz}"

tar --sort=name \
  --mtime=@0 \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  --mode=u=rwX,go=rX \
  --transform=s,^,omatalk/, \
  -cf - \
  daemon \
  systemd \
  pyproject.toml \
  README.md \
  uninstall.sh \
  | gzip -n > "$out"
