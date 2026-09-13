#!/usr/bin/env bash
# Plugin publish: copy install.sh, verify, commit version+installer, push,
# gh release create. Same shape as scripts/release.sh. Does not clobber.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
plugin="$ROOT/plugin"
repo="zerobearing2/omarchy-omatalk-plugin"

if [ ! -e "$plugin/.git" ]; then
  echo "git submodule update --init" >&2
  exit 1
fi
if [ "$(git -C "$plugin" rev-parse --show-toplevel)" != "$plugin" ]; then
  echo "plugin/ is not the submodule" >&2
  exit 1
fi

if [ "$(git -C "$plugin" rev-parse --abbrev-ref HEAD)" != master ]; then
  echo "plugin needs master; git -C plugin switch master" >&2
  exit 1
fi

unexpected=""
while IFS= read -r status; do
  if [ -z "$status" ]; then
    continue
  fi
  path="${status:3}"
  case "$path" in
    manifest.json|install.sh)
      ;;
    *)
      if [ -z "$unexpected" ]; then
        unexpected="$path"
      else
        unexpected="$unexpected $path"
      fi
      ;;
  esac
done <<EOF
$(git -C "$plugin" status --porcelain)
EOF

if [ -n "$unexpected" ]; then
  echo "unexpected dirty paths in plugin/: $unexpected" >&2
  exit 1
fi

git -C "$plugin" fetch origin master --tags

version="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$plugin/manifest.json")"
if [ -z "$version" ]; then
  echo "could not read version from plugin/manifest.json" >&2
  exit 1
fi
tag="v$version"

remote_tag="$(git -C "$plugin" ls-remote --tags origin "refs/tags/$tag")"
if [ -n "$remote_tag" ]; then
  echo "$tag already exists — make plugin-bump first" >&2
  exit 1
fi

if [ "$(git -C "$plugin" merge-base HEAD origin/master)" != "$(git -C "$plugin" rev-parse origin/master)" ]; then
  echo "plugin master is behind origin; pull first" >&2
  exit 1
fi

scripts/plugin-build.sh
scripts/plugin-verify.sh

daemon_tag=""
while IFS= read -r line; do
  case "$line" in
    RELEASE_TAG=*)
      daemon_tag="${line#*:-}"
      daemon_tag="${daemon_tag%\"}"
      daemon_tag="${daemon_tag%\}}"
      ;;
  esac
done < "$plugin/install.sh"

if [ -z "$daemon_tag" ]; then
  echo "could not read RELEASE_TAG from plugin/install.sh" >&2
  exit 1
fi
if ! gh release view "$daemon_tag" --repo zerobearing2/omatalk >/dev/null; then
  echo "Daemon release $daemon_tag is missing — make release first" >&2
  exit 1
fi

git -C "$plugin" add manifest.json install.sh
if git -C "$plugin" diff --cached --quiet; then
  echo "plugin version and install.sh already committed"
else
  git -C "$plugin" commit -m "Release $tag"
fi

git -C "$plugin" push origin master

gh --repo "$repo" release create "$tag" --title "$tag" --generate-notes \
  --target "$(git -C "$plugin" rev-parse HEAD)"

echo "Released plugin $tag"
