#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

for f in manifest.json BarWidget.qml Panel.qml README.md LICENSE preview.png; do
  if [[ ! -f $f ]]; then
    echo "missing $f" >&2
    exit 1
  fi
done

# The plugin never downloads or runs the Daemon installer; setup is the
# site's install page the panel links to.
for f in install.sh uninstall.sh; do
  if [[ -e $f ]]; then
    echo "$f must not live in the plugin tree" >&2
    exit 1
  fi
done

for f in AGENTS.md CONTEXT.md CLAUDE.md; do
  if git ls-files --error-unmatch "$f" >/dev/null 2>&1; then
    echo "workspace-only file must not be tracked: $f" >&2
    exit 1
  fi
done
if git ls-files | grep -E '^(docs/agents/|\.grok/)'; then
  echo "workspace-only paths must not be in the plugin git tree" >&2
  exit 1
fi

link=$(find . -name .git -prune -o -type l -print -quit)
if [[ -n $link ]]; then
  echo "symlinks are not allowed in the plugin tree: $link" >&2
  exit 1
fi

if ! grep -q 'omarchy plugin add https://github.com/zerobearing2/omarchy-omatalk-plugin.git --enable' README.md; then
  echo "README must document the store add command" >&2
  exit 1
fi

id=$(jq -r '.id' manifest.json)
[[ $id == zerobearing.omatalk ]] || {
  echo "manifest id must stay zerobearing.omatalk" >&2
  exit 1
}

ver=$(jq -r '.version' manifest.json)
[[ $ver =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "manifest version must be semver x.y.z, got: $ver" >&2
  exit 1
}

if ! grep -q 'https://github.com/zerobearing2/omatalk' README.md; then
  echo "README must point at the Daemon repository" >&2
  exit 1
fi

if command -v omarchy >/dev/null 2>&1; then
  omarchy plugin validate "$ROOT"
else
  echo "skip: omarchy plugin validate (omarchy not on PATH)"
fi

runner=""
for candidate in /usr/lib/qt6/bin/qmltestrunner "$(command -v qmltestrunner || true)"; do
  if [[ -n $candidate && -x $candidate ]]; then
    runner=$candidate
    break
  fi
done

if [[ -z $runner ]]; then
  if [[ -n ${CI:-} ]]; then
    echo "qmltestrunner is required in CI" >&2
    exit 1
  fi
  echo "skip: qmltestrunner not installed"
  exit 0
fi

QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  "$runner" -input "$ROOT/tests/qml" -import "$ROOT/tests/qml/imports" -o -,txt
