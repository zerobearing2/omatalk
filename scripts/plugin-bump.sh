#!/usr/bin/env bash
# Update plugin/manifest.json version only. Does not commit. Then
# make plugin-release. VERSION=x.y.z to set it explicitly.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
file="$ROOT/plugin/manifest.json"

if [ ! -f "$file" ]; then
  echo "git submodule update --init" >&2
  exit 1
fi

current="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$file")"
if [ -z "$current" ]; then
  echo "could not read version from $file" >&2
  exit 1
fi

if [ -n "${VERSION:-}" ]; then
  new="$VERSION"
else
  major="$(echo "$current" | cut -d. -f1)"
  minor="$(echo "$current" | cut -d. -f2)"
  patch="$(echo "$current" | cut -d. -f3)"
  new="$major.$minor.$((patch + 1))"
fi

sed -i "s/^  \"version\": \".*\",$/  \"version\": \"$new\",/" "$file"
printf 'Bumped %s -> %s in %s (not committed)\n' "$current" "$new" "$file"
