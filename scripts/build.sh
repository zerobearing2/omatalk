#!/usr/bin/env bash
# Build the release binary and pack the runtime tarball. Tag is
# v$(Cargo.toml version). --pack-only packs the binary already in
# target/release (tests/pack.rs uses it).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source scripts/lib.sh

TARBALL="omatalk-x86_64.tar.gz"

pack_only=0
if [ "${1:-}" = "--pack-only" ]; then
  pack_only=1
fi

tag="v$(cargo_version Cargo.toml)"

if [ "$pack_only" -eq 0 ]; then
  cargo build --release --locked
fi

release_dir="${CARGO_TARGET_DIR:-$ROOT/target}/release"
if [ ! -f "$release_dir/omatalk" ]; then
  echo "missing $release_dir/omatalk (run cargo build --release --locked)" >&2
  exit 1
fi

# Reproducible: bytes depend only on the three files. install.sh is not in
# this archive; it is a sibling release asset.
tar --sort=name \
  --mtime=@0 \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  --mode=u=rwX,go=rX \
  --transform=s,^,omatalk/, \
  -cf - \
  -C "$release_dir" omatalk \
  -C "$ROOT/systemd" omatalk.service \
  -C "$ROOT" LICENSE \
  | gzip -n > "$TARBALL"

sha256sum "$TARBALL" > "$TARBALL.sha256"
digest="$(sha256sum "$TARBALL")"
digest="${digest%% *}"

if [ "$pack_only" -eq 1 ]; then
  printf 'packed %s %s\n' "$tag" "$digest"
  exit 0
fi

printf 'built %s %s\n' "$tag" "$digest"
