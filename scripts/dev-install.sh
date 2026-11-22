#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source scripts/lib.sh

MODELS="$HOME/.local/share/omatalk/models"

die() {
  echo "error: $1" >&2
  exit 1
}

newest_tag_version() {
  local tag
  local version
  local best=""
  while IFS= read -r tag; do
    if [[ ! "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)(-rc\.([0-9]+))?$ ]]; then
      continue
    fi
    version="${BASH_REMATCH[1]}${BASH_REMATCH[3]:+rc${BASH_REMATCH[3]}}"
    if [ -z "$best" ] || [ "$(vercmp "$version" "${best#* }")" -gt 0 ]; then
      best="$tag $version"
    fi
  done <<< "$(git tag --list 'v*')"
  printf '%s\n' "$best"
}

for tool in makepkg vercmp sudo pacman; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    die "$tool is required; nothing was changed"
  fi
done

for pin in MODEL_FILE VOICES_FILE; do
  file="$(installer_pin setup.sh "$pin")"
  if [ ! -f "$MODELS/$file" ]; then
    die "$MODELS/$file is missing; run omatalk setup first. Nothing was changed"
  fi
done

newest="$(newest_tag_version)"
if [ -z "$newest" ]; then
  die "no vX.Y.Z or vX.Y.Z-rc.N tag in this clone; run: git fetch --tags. Nothing was changed"
fi
tag="${newest% *}"
base="${newest#* }"
pkgver="$base.r$(git rev-list --count "$tag..HEAD").g$(git rev-parse --short HEAD)"
if [ "$(vercmp "$pkgver" "$base")" -ne 1 ]; then
  die "$pkgver does not sort above $base; nothing was changed"
fi

cargo build --release --locked

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cp packaging/dev/* "$work/"
cp "${CARGO_TARGET_DIR:-$ROOT/target}/release/omatalk" "$work/omatalk"
cp systemd/omatalk.service LICENSE "$work/"
stage_plugin "$ROOT" "$work/stage" "$pkgver"
tar -cf "$work/omarchy-plugin.tar" -C "$work/stage" "$PLUGIN_ID"

cd "$work"
OMATALK_PKGVER="$pkgver" PKGDEST="$work/out" makepkg --noprogressbar
cd "$ROOT"

package=""
for built in "$work"/out/omatalk-bin-*.pkg.tar*; do
  if [ "${built%.sig}" = "$built" ]; then
    package="$built"
  fi
done
if [ -z "$package" ]; then
  die "makepkg built no package"
fi

sudo pacman -U "$package"
printf 'Dev package active: omatalk-bin %s, omatalk %s\n' "$pkgver" "$(/usr/bin/omatalk version)"
