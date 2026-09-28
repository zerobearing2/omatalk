#!/usr/bin/env bash
# Build the release binary, pack the runtime tarball, and write RELEASE_TAG
# + TARBALL_SHA256 into install.sh. Tag is v$(Cargo.toml version).
# --pack-only packs the binary already in target/release and leaves
# install.sh alone (tests/pack.rs uses it).
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

# Reproducible: bytes depend only on the three files. install.sh and
# uninstall.sh are not in this archive; they are sibling release assets.
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
