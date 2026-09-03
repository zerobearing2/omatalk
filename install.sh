#!/usr/bin/env bash
# Omatalk installer: system deps (omarchy-approved), latest GitHub release,
# venv, models, systemd user service, PATH launcher, keybinding line.
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
# Set to a branch name to install unreleased source for testing, bypassing
# the pinned-checksum release path entirely (see step 2 below).
OMATALK_REF="${OMATALK_REF:-}"
RELEASE_BASE="${RELEASE_BASE:-https://github.com/zerobearing2/omatalk/releases/latest/download}"
PLUGIN_REPO="${PLUGIN_REPO:-https://github.com/zerobearing2/omarchy-omatalk-plugin.git}"
MODEL_BASE="${MODEL_BASE:-https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.1}"
MODEL_SHA256="${MODEL_SHA256:-f3a290d384fbb27966d462905c71a46cef9e5fd00516b40df32a0b4afe77ac96}"
VOICES_SHA256="${VOICES_SHA256:-bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d}"
MODEL_FILE="kokoro-v1.0.fp16.onnx"

msg() { printf '\033[1;32m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$1"; }

# Prompts read the terminal when piped via curl | bash; fall back to stdin
# for scripted runs where /dev/tty is unavailable. Tests set ASK_FROM to
# /dev/stdin so `read` cannot steal the developer's tty.
if [ -z "${ASK_FROM:-}" ]; then
  ASK_FROM=/dev/tty
  { : < /dev/tty; } 2>/dev/null || ASK_FROM=/dev/stdin
fi

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

# 2. Source: a specific branch when testing (OMATALK_REF), otherwise always
# the latest GitHub release tarball and its checksum.
mkdir -p "$OMATALK_HOME"
if [ -n "$OMATALK_REF" ]; then
  # A branch is a moving target, so there is no checksum to pin it to —
  # this path trusts HTTPS/GitHub instead, same as any other dev install.
  msg "Downloading branch '$OMATALK_REF' from GitHub (unreleased, unverified)"
  curl -L --fail -o "$OMATALK_HOME/omatalk-src.tar.gz" \
    "https://github.com/zerobearing2/omatalk/archive/refs/heads/$OMATALK_REF.tar.gz"
else
  msg "Downloading latest release from GitHub"
  TS=$(date +%s)
  curl -L --fail -o "$OMATALK_HOME/omatalk-src.tar.gz" "$RELEASE_BASE/omatalk-src.tar.gz?ts=$TS"
  curl -L --fail --silent -o "$OMATALK_HOME/omatalk-src.tar.gz.sha256" "$RELEASE_BASE/omatalk-src.tar.gz.sha256?ts=$TS"
  (cd "$OMATALK_HOME" && sha256sum -c omatalk-src.tar.gz.sha256 --quiet)
fi

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

# 9. Bar plugin (Omarchy only). A git checkout (store add) is left alone so
# `omarchy plugin update` keeps working. A legacy copy is still replaced from
# the tarball the way `omarchy plugin add` stages files: one rename, then
# rescan, wait until the shell reports the plugin, then enable. A missing
# directory is `omarchy plugin add`; if that fails, the tarball copy is the
# fallback.
stage_plugin_from_tarball() {
  local plugins_dir="$1"
  local plugin_dir="$2"
  local stage="$plugins_dir/.omatalk.add.$$"
  mkdir -p "$plugins_dir"
  rm -rf "$stage"
  cp -r "$OMATALK_HOME/src/plugin" "$stage"
  rm -rf "$plugin_dir"
  mv "$stage" "$plugin_dir"
}

discover_and_enable_plugin() {
  omarchy-shell shell rescanPlugins >/dev/null

  local plugin_seen=0
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
}

prompt_plugin_shell_restart() {
  warn "The bar plugin was already installed before this run — its icon may need a shell restart to show the update."
  # A closed/non-interactive stdin (no answer to read) must not abort the
  # rest of the install under set -e — default to declining the restart.
  local answer
  read -r -p "Restart the Omarchy shell now to apply it? [y/N] " answer < "$ASK_FROM" || answer="n"
  if [[ "$answer" =~ ^[Yy]$ ]]; then
    omarchy restart shell
  else
    warn "Skipped. If the bar icon looks stale, run: omarchy restart shell"
  fi
}

if command -v omarchy >/dev/null 2>&1; then
  plugins_dir="$HOME/.config/omarchy/plugins"
  plugin_dir="$plugins_dir/zerobearing.omatalk"
  if [ -e "$plugin_dir/.git" ]; then
    msg "Omarchy bar plugin is a git checkout; leaving it in place"
  elif [ -d "$plugin_dir" ]; then
    msg "Replacing copy-based Omarchy bar plugin"
    stage_plugin_from_tarball "$plugins_dir" "$plugin_dir"
    discover_and_enable_plugin
    prompt_plugin_shell_restart
  else
    msg "Installing Omarchy bar plugin"
    if omarchy plugin add "$PLUGIN_REPO" --enable --yes >/dev/null 2>&1; then
      :
    else
      warn "Could not add $PLUGIN_REPO; installing from the release tarball"
      stage_plugin_from_tarball "$plugins_dir" "$plugin_dir"
      discover_and_enable_plugin
    fi
  fi
fi

"$HOME/.local/bin/omatalk" speak "Welcome to omatalk!" >/dev/null 2>&1

msg "Done. Select text and press F8, or run: omatalk speak|stop|status|upgrade"
