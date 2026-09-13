#!/usr/bin/env bash
# Rewrite plugin/Panel.qml's installerUrl + installerSha256 to a tagged
# omatalk release (default: latest) or a 40-character commit. Does not commit.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PANEL="$ROOT/plugin/Panel.qml"
REPO="${OMATALK_REPO:-zerobearing2/omatalk}"
REF="${1:-}"

if [[ ! -f $PANEL ]]; then
  echo "missing $PANEL" >&2
  exit 1
fi

require_gh() {
  if ! command -v gh >/dev/null 2>&1; then
    echo "gh is required to resolve omatalk tags" >&2
    exit 1
  fi
}

resolve_tag_commit() {
  local tag=$1
  local sha type
  require_gh
  sha=$(gh api "repos/$REPO/git/ref/tags/$tag" --jq .object.sha)
  type=$(gh api "repos/$REPO/git/ref/tags/$tag" --jq .object.type)
  if [[ $type == tag ]]; then
    gh api "repos/$REPO/git/tags/$sha" --jq .object.sha
  else
    printf '%s\n' "$sha"
  fi
}

if [[ -z $REF ]]; then
  require_gh
  REF=$(gh release view --repo "$REPO" --json tagName --jq .tagName)
fi

if [[ $REF =~ ^[0-9a-f]{40}$ ]]; then
  COMMIT=$REF
elif [[ $REF =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  [[ $REF == v* ]] || REF="v$REF"
  COMMIT=$(resolve_tag_commit "$REF")
else
  echo "REF must be a vX.Y.Z tag or a 40-character commit, got: $REF" >&2
  exit 1
fi

URL="https://raw.githubusercontent.com/$REPO/$COMMIT/install.sh"
TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT
curl -fsS --proto '=https' --tlsv1.2 --max-redirs 0 -o "$TMP" "$URL"
if ! head -n1 "$TMP" | grep -qx '#!/usr/bin/env bash'; then
  echo "pinned install.sh is not a bash script: $URL" >&2
  exit 1
fi
HASH=$(sha256sum "$TMP" | awk '{print $1}')
if [[ ! $HASH =~ ^[0-9a-f]{64}$ ]]; then
  echo "could not hash $URL" >&2
  exit 1
fi

python3 - "$PANEL" "$URL" "$HASH" <<'PY'
import pathlib, re, sys
path, url, digest = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
text = path.read_text()
url_re = re.compile(r'^(  readonly property string installerUrl: ")[^"]+(")$', re.M)
hash_re = re.compile(r'^(  readonly property string installerSha256: ")[^"]+(")$', re.M)
new, n_url = url_re.subn(lambda m: m.group(1) + url + m.group(2), text, count=1)
new, n_hash = hash_re.subn(lambda m: m.group(1) + digest + m.group(2), new, count=1)
if n_url != 1 or n_hash != 1:
    sys.exit(f"expected one installerUrl and one installerSha256 in {path}")
if new != text:
    path.write_text(new)
PY

echo "Pinned $REF ($COMMIT)"
echo "  $URL"
echo "  $HASH"
