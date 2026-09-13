#!/usr/bin/env bash
# Omatalk installer: system deps (omarchy-approved), a pinned GitHub
# release tarball, venv, models, systemd user unit, PATH launcher. The
# bar plugin is `omarchy plugin add` of PLUGIN_REPO, not files from this
# tarball. make build rewrites RELEASE_TAG and TARBALL_SHA256.
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
RELEASE_TAG="${RELEASE_TAG:-v0.5.0}"
TARBALL_SHA256="${TARBALL_SHA256:-af786dd98f4a1282e70a4bfea1edfa15e7c248ffa3ba2b75a71cf9a706ff7fc2}"
RELEASE_BASE="${RELEASE_BASE:-https://github.com/zerobearing2/omatalk/releases/download/${RELEASE_TAG}}"
PLUGIN_REPO="${PLUGIN_REPO:-https://github.com/zerobearing2/omarchy-omatalk-plugin.git}"
MODEL_BASE="${MODEL_BASE:-https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.1}"
MODEL_SHA256="${MODEL_SHA256:-f3a290d384fbb27966d462905c71a46cef9e5fd00516b40df32a0b4afe77ac96}"
VOICES_SHA256="${VOICES_SHA256:-bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d}"
MODEL_FILE="kokoro-v1.0.fp16.onnx"

msg() {
  printf '\033[1;32m==>\033[0m %s\n' "$1"
}

warn() {
  printf '\033[1;33m==>\033[0m %s\n' "$1"
}

download_model() {
  local file="$1"
  local sha256="$2"
  local path="$OMATALK_HOME/models/$file"

  if [ -s "$path" ]; then
    if echo "$sha256  $path" | sha256sum -c --quiet; then
      return
    fi
  fi

  rm -f "$path"
  msg "Downloading $file (~185MB total) — this can take a few minutes depending on your connection"
  download "$MODEL_BASE/$file" "$path" --progress-bar
  echo "$sha256  $path" | sha256sum -c --quiet
}

download() {
  local url="$1"
  local dest="$2"
  shift 2
  case "$url" in
    https://*)
      curl --fail --proto '=https' --tlsv1.2 --location --max-redirs 5 "$@" -o "$dest" "$url"
      ;;
    *)
      curl --fail --location --max-redirs 5 "$@" -o "$dest" "$url"
      ;;
  esac
}

add_bar_plugin() {
  if omarchy plugin add "$PLUGIN_REPO" --enable --yes >/dev/null 2>&1; then
    return
  else
    warn "Could not add $PLUGIN_REPO; F8 still speaks. Add the plugin with: omarchy plugin add $PLUGIN_REPO --enable"
  fi
}

# 1. System dependencies. Omarchy only — omarchy pkg add, never pacman.
if ! command -v omarchy >/dev/null 2>&1; then
  msg "Omatalk requires Omarchy. Install Omarchy, then rerun."
  exit 1
fi
PKG_DEPS=(python curl pipewire wl-clipboard uv)
if omarchy pkg present "${PKG_DEPS[@]}"; then
  msg "System dependencies present: ${PKG_DEPS[*]}"
else
  msg "Installing missing packages via omarchy pkg add"
  omarchy pkg add "${PKG_DEPS[@]}"
fi

# 2. Pinned GitHub release tarball. Digest is in this script, not fetched
# beside the artifact.
mkdir -p "$OMATALK_HOME"
msg "Downloading $RELEASE_TAG from GitHub"
download "$RELEASE_BASE/omatalk-src.tar.gz" "$OMATALK_HOME/omatalk-src.tar.gz"
echo "$TARBALL_SHA256  $OMATALK_HOME/omatalk-src.tar.gz" | sha256sum -c --quiet

# 3. Models (~185MB, skipped when their checksums match). fp16 half-size
# export: spectral correlation 0.999 against fp32 — audibly identical.
# Checksums pin the exact artifacts we validated by listening. Deliberately
# before the stop: the daemon only reads models at startup, so fetching
# them while it still runs keeps its downtime to the venv + service swap
# (and a failed download leaves the old daemon untouched).
mkdir -p "$OMATALK_HOME/models"
download_model "$MODEL_FILE" "$MODEL_SHA256"
download_model "voices-v1.0.bin" "$VOICES_SHA256"

# 4. Stop the daemon before replacing the venv it runs from.
msg "Stopping the current daemon"
set +e
systemctl --user stop omatalk.service 2>/dev/null
stop_status=$?
set -e
if [ "$stop_status" -ne 0 ]; then
  if [ "$stop_status" -ne 5 ]; then
    msg "Could not stop the current daemon; refusing to replace its files"
    exit "$stop_status"
  fi
fi
rm -rf "$OMATALK_HOME/src"
mkdir -p "$OMATALK_HOME/src"
tar -xzf "$OMATALK_HOME/omatalk-src.tar.gz" -C "$OMATALK_HOME/src" --strip-components=1
rm -f "$OMATALK_HOME/omatalk-src.tar.gz"

# 5. Python environment (uv; fast installs, kokoro-onnx bundles its own phonemizer).
# --clear makes reinstalls and version upgrades work over an existing install.
msg "Setting up Python environment with uv"
uv venv --quiet --clear "$OMATALK_HOME/venv"
uv pip install --quiet --python "$OMATALK_HOME/venv/bin/python" "$OMATALK_HOME/src"

# 6. Client and systemd user unit.
msg "Installing systemd user unit"
mkdir -p "$HOME/.config/systemd/user" "$HOME/.local/bin"
cp "$OMATALK_HOME/src/systemd/omatalk.service" "$HOME/.config/systemd/user/"
cp "$OMATALK_HOME/venv/bin/omatalk" "$HOME/.local/bin/omatalk"

systemctl --user daemon-reload
systemctl --user enable --now omatalk.service

# 7. Keybindings are user-owned; the installer only prints the command.
need_bind=0
if [ ! -f "$HOME/.config/hypr/bindings.lua" ]; then
  need_bind=1
else
  if ! grep -q omatalk "$HOME/.config/hypr/bindings.lua"; then
    need_bind=1
  fi
fi
if [ "$need_bind" -eq 1 ]; then
  msg "To bind F8, paste this command (safe to re-run):"
  cat <<'EOF'
    grep -q omatalk ~/.config/hypr/bindings.lua || printf '\no.bind("F8", "Omatalk", "omatalk speak")\n' >> ~/.config/hypr/bindings.lua; hyprctl reload
EOF
fi

# 8. Welcome through the freshly installed daemon — proves the whole
# pipeline (service, socket, warm model, audio) works end to end.
started=0
for _ in $(seq 1 30); do
  if "$HOME/.local/bin/omatalk" status >/dev/null 2>&1; then
    started=1
    break
  fi
  sleep 1
done
if [ "$started" -ne 1 ]; then
  msg "Daemon did not start; check: journalctl --user -u omatalk"
  exit 1
fi

# 9. Bar plugin. QML lives in PLUGIN_REPO, not this tarball. Official tools
# only: add when missing, remove-then-add to convert a legacy copy into a
# git checkout, leave an existing git checkout for `omarchy plugin update`.
# A failed add does not fail the Daemon install.
plugin_dir="$HOME/.config/omarchy/plugins/zerobearing.omatalk"
if [ -e "$plugin_dir/.git" ]; then
  msg "Omarchy bar plugin is a git checkout; leaving it in place"
else
  if [ -d "$plugin_dir" ]; then
    msg "Replacing copy-based Omarchy bar plugin with $PLUGIN_REPO"
    if omarchy plugin remove zerobearing.omatalk --yes >/dev/null 2>&1; then
      add_bar_plugin
    else
      warn "Could not remove the copy-based plugin; F8 still speaks. Convert it with: omarchy plugin remove zerobearing.omatalk --yes && omarchy plugin add $PLUGIN_REPO --enable"
    fi
  else
    msg "Installing Omarchy bar plugin"
    add_bar_plugin
  fi
fi

"$HOME/.local/bin/omatalk" speak "Welcome to omatalk!" >/dev/null 2>&1

msg "Done. Select text and press F8, or run: omatalk speak|stop|status|upgrade"
