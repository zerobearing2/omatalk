#!/usr/bin/env bash
# Daemon publish: test, lint, build, commit version+pin, push, gh release create
# from the tarball just packed. Does not clobber an existing tag. A version
# with a prerelease suffix (0.9.0-dev.N) publishes a GitHub prerelease, so
# releases/latest (the site installer and omatalk upgrade) keeps serving the
# last stable release.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source scripts/lib.sh

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
    Cargo.toml|Cargo.lock|install.sh)
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

version="$(cargo_version Cargo.toml)"
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

cargo test --locked
make lint
scripts/build.sh

git add Cargo.toml Cargo.lock install.sh
if git diff --cached --quiet; then
  echo "version and pin already committed"
else
  git commit -m "Release $tag"
fi

git push origin master

if [ ! -f omatalk-x86_64.tar.gz ]; then
  echo "missing omatalk-x86_64.tar.gz after build" >&2
  exit 1
fi
if [ ! -f omatalk-x86_64.tar.gz.sha256 ]; then
  echo "missing omatalk-x86_64.tar.gz.sha256 after build" >&2
  exit 1
fi

prerelease=()
if [[ "$version" == *-* ]]; then
  prerelease=(--prerelease)
fi

gh release create "$tag" --title "$tag" --generate-notes "${prerelease[@]}" \
  --target "$(git rev-parse HEAD)" \
  omatalk-x86_64.tar.gz omatalk-x86_64.tar.gz.sha256 install.sh uninstall.sh

echo "Released $tag"
