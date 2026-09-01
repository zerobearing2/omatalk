#!/usr/bin/env bash
# Omatalk installer: system deps (omarchy-approved), latest release from the
# site, venv, models, systemd user service, PATH launcher, keybinding line.
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
SITE_BASE="${SITE_BASE:-https://omatalk.zerobearing.com}"
MODEL_BASE="https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.1"
MODEL_SHA256="f3a290d384fbb27966d462905c71a46cef9e5fd00516b40df32a0b4afe77ac96"
VOICES_SHA256="bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d"
MODEL_FILE="kokoro-v1.0.fp16.onnx"

msg() { printf '\033[1;32m==>\033[0m %s\n' "$1"; }

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

# 2. Source: always the latest release tarball, mirrored on the site by
# the Pages workflow. The CDN may cache aggressively (observed 4h on the
# tarball), so both files are cache-busted with one timestamp — they then
# come straight from the current deploy as a matching pair.
msg "Downloading latest release from $SITE_BASE"
TS=$(date +%s)
mkdir -p "$OMATALK_HOME/src"
curl -L --fail -o "$OMATALK_HOME/omatalk-src.tar.gz" "$SITE_BASE/omatalk-src.tar.gz?ts=$TS"
curl -L --fail --silent -o "$OMATALK_HOME/omatalk-src.tar.gz.sha256" "$SITE_BASE/omatalk-src.tar.gz.sha256?ts=$TS"
(cd "$OMATALK_HOME" && sha256sum -c omatalk-src.tar.gz.sha256 --quiet)
tar -xzf "$OMATALK_HOME/omatalk-src.tar.gz" -C "$OMATALK_HOME/src" --strip-components=1
rm -f "$OMATALK_HOME/omatalk-src.tar.gz" "$OMATALK_HOME/omatalk-src.tar.gz.sha256"

# 3. Python environment (uv; fast installs, kokoro-onnx bundles its own phonemizer).
# --clear makes reinstalls and version upgrades work over an existing install.
msg "Setting up Python environment with uv"
uv venv --quiet --clear "$OMATALK_HOME/venv"
uv pip install --quiet --python "$OMATALK_HOME/venv/bin/python" "$OMATALK_HOME/src"

# 4. Models (~185MB, skipped if already present). fp16 half-size export:
# spectral correlation 0.999 against fp32 — audibly identical. Checksums
# pin the exact artifacts we validated by listening.
mkdir -p "$OMATALK_HOME/models"
for f in "$MODEL_FILE" voices-v1.0.bin; do
  if [ ! -s "$OMATALK_HOME/models/$f" ]; then
    msg "Downloading $f (~185MB total) — this can take a few minutes depending on your connection"
    curl -L --fail --progress-bar -o "$OMATALK_HOME/models/$f" "$MODEL_BASE/$f"
  fi
done
msg "Verifying model checksums"
echo "${MODEL_SHA256}  $OMATALK_HOME/models/$MODEL_FILE" | sha256sum -c --quiet
echo "${VOICES_SHA256}  $OMATALK_HOME/models/voices-v1.0.bin" | sha256sum -c --quiet

# 5. systemd user service: warm daemon from login.
msg "Installing systemd user service"
mkdir -p "$HOME/.config/systemd/user" "$HOME/.local/bin"
cp "$OMATALK_HOME/src/systemd/omatalk.service" "$HOME/.config/systemd/user/"
systemctl --user daemon-reload
systemctl --user enable --now omatalk.service

# 6. Client on PATH.
cp "$OMATALK_HOME/venv/bin/omatalk" "$HOME/.local/bin/omatalk"

# 6b. Bar plugin (Omarchy only).
if command -v omarchy >/dev/null 2>&1; then
  msg "Installing Omarchy bar plugin"
  rm -rf "$HOME/.config/omarchy/plugins/zerobearing.omatalk"
  cp -r "$OMATALK_HOME/src/plugin" "$HOME/.config/omarchy/plugins/zerobearing.omatalk"
  omarchy-shell shell rescanPlugins
  plugin_enabled=0
  for _ in $(seq 1 50); do
    if omarchy plugin enable zerobearing.omatalk >/dev/null 2>&1; then
      plugin_enabled=1
      break
    fi
    sleep 0.1
  done
  if (( ! plugin_enabled )); then
    msg "Could not enable the Omarchy bar plugin; run: omarchy plugin enable zerobearing.omatalk"
    exit 1
  fi
fi

# 7. Keybindings are user-owned; the installer only prints the command.
msg "To bind F8, paste this command (safe to re-run):"
cat <<'EOF'
  grep -q omatalk ~/.config/hypr/bindings.lua || printf '\no.bind("F8", "Omatalk", "omatalk speak")\n' >> ~/.config/hypr/bindings.lua; hyprctl reload
EOF

# 8. Welcome through the freshly installed daemon — proves the whole
# pipeline (service, socket, warm model, audio) works end to end.
for _ in $(seq 1 30); do
  "$HOME/.local/bin/omatalk" status >/dev/null 2>&1 && break
  sleep 1
done
if "$HOME/.local/bin/omatalk" status >/dev/null 2>&1; then
  "$HOME/.local/bin/omatalk" speak "Welcome to omatalk!" >/dev/null 2>&1
else
  msg "Daemon not up yet; check: journalctl --user -u omatalk"
fi

msg "Done. Select text and press F8, or run: omatalk speak|stop|status"
