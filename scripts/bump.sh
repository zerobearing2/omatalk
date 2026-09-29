#!/usr/bin/env bash
# Update the Cargo.toml version and Cargo.lock. Does not commit. Then make release.
# 0.9.0-dev.3 bumps to 0.9.0-dev.4; 0.9.0 bumps to 0.9.1.
# VERSION=x.y.z (or x.y.z-dev.N) sets it explicitly.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source scripts/lib.sh

file="Cargo.toml"
current="$(cargo_version "$file")"

if [ -n "${VERSION:-}" ]; then
  new="$VERSION"
elif [[ "$current" =~ ^([0-9]+\.[0-9]+\.[0-9]+)-dev\.([0-9]+)$ ]]; then
  new="${BASH_REMATCH[1]}-dev.$((BASH_REMATCH[2] + 1))"
elif [[ "$current" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  new="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}.$((BASH_REMATCH[3] + 1))"
else
  echo "$file version $current is not x.y.z or x.y.z-dev.N; set VERSION" >&2
  exit 1
fi

if [[ ! "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-dev\.[0-9]+)?$ ]]; then
  echo "$new: version must be x.y.z or x.y.z-dev.N" >&2
  exit 1
fi

# Only inside [package]: the range ends at the next table header.
sed -i "/^\[package\]$/,/^\[/ s/^version = \".*\"$/version = \"$new\"/" "$file"
cargo update --workspace --offline --quiet
printf 'Bumped %s -> %s in %s and Cargo.lock (not committed)\n' "$current" "$new" "$file"
