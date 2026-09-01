#!/usr/bin/env bash
# Omatalk installer: system deps (omarchy-approved), latest GitHub release,
# venv, models, systemd user service, PATH launcher, keybinding line.
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
RELEASE_BASE="${RELEASE_BASE:-https://github.com/zerobearing2/omatalk/releases/latest/download}"
MODEL_BASE="${MODEL_BASE:-https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.1}"
MODEL_SHA256="${MODEL_SHA256:-f3a290d384fbb27966d462905c71a46cef9e5fd00516b40df32a0b4afe77ac96}"
VOICES_SHA256="${VOICES_SHA256:-bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d}"
MODEL_FILE="kokoro-v1.0.fp16.onnx"

msg() { printf '\033[1;32m==>\033[0m %s\n' "$1"; }

download_model() {
  local file="$1"
  local sha256="$2"
  local path="$OMATALK_HOME/models/$file"

  if [ -s "$path" ] && echo "$sha256  $path" | sha256sum -c --quiet; then
    return
  fi

  rm -f "$path"
  msg "Downloading $file (~185MB total) — this can take a few minutes depending on your connection"
  curl -L --fail --progress-bar -o "$path" "$MODEL_BASE/$file"
  echo "$sha256  $path" | sha256sum -c --quiet
}

# 1. System dependencies. On Omarchy, check and install via the omarchy CLI.
PKG_DEPS=(python curl pipewire wl-clipboard uv)
if command -v omarchy >/dev/null 2>&1; then
  if omarchy pkg present "${PKG_DEPS[@]}"; then
    msg "System dependencies present: ${PKG_DEPS[*]}"
  else
    msg "Installing missing packages via omarchy pkg add"
    omarchy pkg add "${PKG_DEPS[@]}"
  fi
else
  for pkg in "${PKG_DEPS[@]}"; do
    pacman -Q "$pkg" >/dev/null 2>&1 || {
      msg "Missing: $pkg — install it (pacman -S $pkg) and rerun."
      exit 1
    }
  done
fi

# 2. Source: always the latest GitHub release tarball and checksum.
msg "Downloading latest release from GitHub"
TS=$(date +%s)
mkdir -p "$OMATALK_HOME"
curl -L --fail -o "$OMATALK_HOME/omatalk-src.tar.gz" "$RELEASE_BASE/omatalk-src.tar.gz?ts=$TS"
curl -L --fail --silent -o "$OMATALK_HOME/omatalk-src.tar.gz.sha256" "$RELEASE_BASE/omatalk-src.tar.gz.sha256?ts=$TS"
(cd "$OMATALK_HOME" && sha256sum -c omatalk-src.tar.gz.sha256 --quiet)

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
stop_status=0
systemctl --user stop omatalk.service 2>/dev/null || stop_status=$?
if [ "$stop_status" -ne 0 ] && [ "$stop_status" -ne 5 ]; then
  msg "Could not stop the current daemon; refusing to replace its files"
  exit "$stop_status"
fi
rm -rf "$OMATALK_HOME/src"
mkdir -p "$OMATALK_HOME/src"
tar -xzf "$OMATALK_HOME/omatalk-src.tar.gz" -C "$OMATALK_HOME/src" --strip-components=1
rm -f "$OMATALK_HOME/omatalk-src.tar.gz" "$OMATALK_HOME/omatalk-src.tar.gz.sha256"

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
if [ ! -f "$HOME/.config/hypr/bindings.lua" ] || ! grep -q omatalk "$HOME/.config/hypr/bindings.lua"; then
  msg "To bind F8, paste this command (safe to re-run):"
  cat <<'EOF'
    grep -q omatalk ~/.config/hypr/bindings.lua || printf '\no.bind("F8", "Omatalk", "omatalk speak")\n' >> ~/.config/hypr/bindings.lua; hyprctl reload
EOF
fi

# 8. Welcome through the freshly installed daemon — proves the whole
# pipeline (service, socket, warm model, audio) works end to end.
for _ in $(seq 1 30); do
  "$HOME/.local/bin/omatalk" status >/dev/null 2>&1 && break
  sleep 1
done
if ! "$HOME/.local/bin/omatalk" status >/dev/null 2>&1; then
  msg "Daemon did not start; check: journalctl --user -u omatalk"
  exit 1
fi

# 9. Bar plugin (Omarchy only). This mirrors what `omarchy plugin add` does
# (/usr/share/omarchy/bin/omarchy-plugin-add): stage the files, move them into
# place with one rename, rescan, wait until the shell reports the plugin
# discovered, then enable it once.
#
# The rename is the part that matters. Copying file by file into the live
# plugins directory fires an inotify event per file, and the shell debounces a
# reload 150ms after any of them, so a copy slower than that gets scanned while
# it is half written. Staging outside the directory keeps the plugin whole:
# absent, then complete, never partial.
#
# Waiting for discovery replaces guesswork about how long a reload takes. The
# shell refuses to enable a plugin it has not scanned yet
# (PluginRegistry.setEnabled), so asking it what it knows is the signal.
if command -v omarchy >/dev/null 2>&1; then
  msg "Installing Omarchy bar plugin"
  plugins_dir="$HOME/.config/omarchy/plugins"
  stage="$plugins_dir/.omatalk.add.$$"
  mkdir -p "$plugins_dir"
  rm -rf "$stage"
  cp -r "$OMATALK_HOME/src/plugin" "$stage"
  rm -rf "$plugins_dir/zerobearing.omatalk"
  mv "$stage" "$plugins_dir/zerobearing.omatalk"
  omarchy-shell shell rescanPlugins >/dev/null

  plugin_seen=0
  for _ in $(seq 1 40); do
    if omarchy plugin list --json |
      jq -e 'any(.[]; .id == "zerobearing.omatalk")' >/dev/null 2>&1; then
      plugin_seen=1
      break
    fi
    sleep 0.05
  done
  if (( ! plugin_seen )); then
    msg "Omarchy did not discover the bar plugin; run: omarchy plugin enable zerobearing.omatalk"
    exit 1
  fi

  if ! omarchy plugin enable zerobearing.omatalk >/dev/null 2>&1; then
    msg "Could not enable the Omarchy bar plugin; run: omarchy plugin enable zerobearing.omatalk"
    exit 1
  fi
fi

"$HOME/.local/bin/omatalk" speak "Welcome to omatalk!" >/dev/null 2>&1

msg "Done. Select text and press F8, or run: omatalk speak|stop|status|upgrade"
