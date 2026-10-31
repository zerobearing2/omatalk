#!/usr/bin/env bash
# Publish a release from master: a tag plus its GitHub Release, never a push
# to master or any branch.
#
#   VERSION=x.y.z scripts/release.sh rc      publishes vX.Y.Z-rc.N as a prerelease
#   VERSION=x.y.z scripts/release.sh stable  publishes vX.Y.Z as latest
#
# The version change, the install.sh pin, and the release commit exist only in a
# temporary worktree. The commit is tagged, and only the tag is pushed. A
# stable release requires that the highest RC was built from this exact master.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="zerobearing2/omatalk"
TARBALL="omatalk-x86_64.tar.gz"

cd "$ROOT"
source scripts/lib.sh

die() {
  echo "$1" >&2
  exit 1
}

mode="${1:-}"
version="${VERSION:-}"

# Preflight: nothing changes until all of it passes.
if [ "$mode" != rc ] && [ "$mode" != stable ]; then
  die "usage: VERSION=x.y.z scripts/release.sh rc|stable"
fi

if [ -z "$version" ]; then
  die "VERSION is required, as x.y.z"
fi

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  die "VERSION must be exactly x.y.z, got: $version"
fi

branch="$(git rev-parse --abbrev-ref HEAD)"
if [ "$branch" != master ]; then
  die "need branch master, on $branch; git switch master"
fi

if [ -n "$(git status --porcelain)" ]; then
  die "working tree is not clean; commit or stash first"
fi

if ! git fetch --quiet origin master --tags; then
  die "git fetch origin master --tags failed"
fi

head="$(git rev-parse HEAD)"
origin_head="$(git rev-parse refs/remotes/origin/master)"
if [ "$head" != "$origin_head" ]; then
  die "master is not origin/master; pull or push first"
fi

base_tag="v$version"

if [ -n "$(git ls-remote --tags origin "refs/tags/$base_tag")" ]; then
  die "$base_tag already exists on origin"
fi

rc_refs="$(git ls-remote --tags origin "refs/tags/$base_tag-rc.*")"
highest_rc=0
while read -r _ ref; do
  if [ -z "$ref" ]; then
    continue
  fi
  n="${ref##*-rc.}"
  if [[ ! "$n" =~ ^[0-9]+$ ]]; then
    continue
  fi
  if [ "$ref" != "refs/tags/$base_tag-rc.$n" ]; then
    continue
  fi
  if [ $((10#$n)) -gt "$highest_rc" ]; then
    highest_rc=$((10#$n))
  fi
done <<EOF
$rc_refs
EOF

if [ "$mode" = rc ]; then
  release_version="$version-rc.$((highest_rc + 1))"
else
  release_version="$version"
  if [ "$highest_rc" -eq 0 ]; then
    die "no $base_tag-rc.N tag on origin; make rc VERSION=$version first"
  fi
  rc_tag="$base_tag-rc.$highest_rc"
  rc_parent="$(git rev-parse "refs/tags/$rc_tag^")"
  if [ "$rc_parent" != "$head" ]; then
    die "master moved since $rc_tag; cut another RC: make rc VERSION=$version"
  fi
fi

tag="v$release_version"
if git rev-parse --quiet --verify "refs/tags/$tag" > /dev/null; then
  die "local tag $tag already exists; git tag -d $tag"
fi

work_parent="$(mktemp -d)"
work="$work_parent/tree"
tag_created=0
tag_pushed=0

cleanup() {
  set +e
  cd "$ROOT"
  if [ "$tag_created" -eq 1 ] && [ "$tag_pushed" -eq 0 ]; then
    git tag -d "$tag" > /dev/null
  fi
  if [ -d "$work" ]; then
    git worktree remove --force "$work"
  fi
  git worktree prune
  rm -rf "$work_parent"
}
trap cleanup EXIT

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

git worktree add --detach --quiet "$work" "$head"
cd "$work"

# Only inside [package]: the range ends at the next table header.
sed -i "/^\[package\]$/,/^\[/ s/^version = \".*\"$/version = \"$release_version\"/" Cargo.toml
if [ "$(cargo_version Cargo.toml)" != "$release_version" ]; then
  die "could not set the [package] version to $release_version"
fi
cargo update --workspace --offline --quiet

cargo test --locked
make lint
scripts/build.sh

for asset in "$TARBALL" "$TARBALL.sha256" install.sh uninstall.sh; do
  if [ ! -f "$asset" ]; then
    die "missing $asset after build"
  fi
done

git add Cargo.toml Cargo.lock install.sh
git commit --quiet -m "Release $tag"
git tag "$tag"
tag_created=1

if ! git push --quiet origin "refs/tags/$tag"; then
  die "could not push $tag; nothing was published"
fi
tag_pushed=1

if latest_before="$(gh api "repos/$REPO/releases/latest" --jq .tag_name)"; then
  :
else
  latest_before=""
fi

if [ "$mode" = rc ]; then
  channel=(--prerelease --latest=false)
else
  channel=(--latest)
fi

if ! gh release create "$tag" --repo "$REPO" --verify-tag --title "$tag" \
  --generate-notes "${channel[@]}" \
  "$TARBALL" "$TARBALL.sha256" install.sh uninstall.sh; then
  echo "$tag is pushed but its GitHub Release was not created." >&2
  echo "Retry the gh release create, or delete the tag: git push origin :refs/tags/$tag" >&2
  exit 1
fi

if ! latest_after="$(gh api "repos/$REPO/releases/latest" --jq .tag_name)"; then
  die "could not read releases/latest after publishing $tag"
fi

if [ "$mode" = rc ]; then
  if [ "$latest_after" != "$latest_before" ]; then
    die "$tag is published, but releases/latest moved from '$latest_before' to '$latest_after'"
  fi
else
  if [ "$latest_after" != "$tag" ]; then
    die "$tag is published, but releases/latest is '$latest_after', not $tag"
  fi
fi

echo "Released $tag"
if [ "$mode" = rc ]; then
  echo "Install it with:"
  printf 'bash -c "$(curl -fsSL https://github.com/%s/releases/download/%s/install.sh)"\n' "$REPO" "$tag"
fi
