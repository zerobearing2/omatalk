#!/usr/bin/env bash
# Daemon publish: build, verify, commit version+pin, push, gh release create
# from the tarball just packed. Does not clobber an existing tag.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ "$(git rev-parse --abbrev-ref HEAD)" != master ]; then
  echo "need master; git switch master" >&2
  exit 1
fi

unexpected=""
while IFS= read -r status; do
  if [ -z "$status" ]; then
    continue
  fi
  path="${status:3}"
  case "$path" in
    pyproject.toml|install.sh)
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
$(git status --porcelain)
EOF

if [ -n "$unexpected" ]; then
  echo "unexpected dirty paths: $unexpected" >&2
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
  echo "$tag already exists — make bump first" >&2
  exit 1
fi

if [ "$(git merge-base HEAD origin/master)" != "$(git rev-parse origin/master)" ]; then
  echo "master is behind origin; pull first" >&2
  exit 1
fi

scripts/build.sh
scripts/verify.sh

git add pyproject.toml install.sh
if git diff --cached --quiet; then
  echo "version and pin already committed"
else
  git commit -m "Release $tag"
fi

git push origin master

if [ ! -f omatalk-src.tar.gz ]; then
  echo "missing omatalk-src.tar.gz after build" >&2
  exit 1
fi
if [ ! -f omatalk-src.tar.gz.sha256 ]; then
  echo "missing omatalk-src.tar.gz.sha256 after build" >&2
  exit 1
fi

gh release create "$tag" --title "$tag" --generate-notes \
  --target "$(git rev-parse HEAD)" \
  omatalk-src.tar.gz omatalk-src.tar.gz.sha256 install.sh uninstall.sh

echo "Released $tag"
