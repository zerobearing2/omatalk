#!/usr/bin/env bash
# Omatalk installer: system deps (omarchy-approved), latest release from the
# site, venv, models, systemd user service, PATH launcher, keybinding line.
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
SITE_BASE="${SITE_BASE:-https://omatalk.dev}"
MODEL_BASE="https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0"
MODEL_SHA256="7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5"
VOICES_SHA256="bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d"

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
# the Pages workflow.
msg "Downloading latest release from $SITE_BASE"
mkdir -p "$OMATALK_HOME/src"
curl -L --fail -o "$OMATALK_HOME/omatalk-src.tar.gz" "$SITE_BASE/omatalk-src.tar.gz"
curl -L --fail --silent -o "$OMATALK_HOME/omatalk-src.tar.gz.sha256" "$SITE_BASE/omatalk-src.tar.gz.sha256"
(cd "$OMATALK_HOME" && sha256sum -c omatalk-src.tar.gz.sha256 --quiet)
tar -xzf "$OMATALK_HOME/omatalk-src.tar.gz" -C "$OMATALK_HOME/src" --strip-components=1
rm -f "$OMATALK_HOME/omatalk-src.tar.gz" "$OMATALK_HOME/omatalk-src.tar.gz.sha256"

# 3. Python environment (uv; fast installs, kokoro-onnx bundles its own phonemizer).
msg "Setting up Python environment with uv"
uv venv --quiet "$OMATALK_HOME/venv"
uv pip install --quiet --python "$OMATALK_HOME/venv/bin/python" "$OMATALK_HOME/src"

# 4. Models (~340MB, skipped if already present). Checksums pin the exact
# artifacts we validated by listening — a re-exported "same" model is not
# the same model.
mkdir -p "$OMATALK_HOME/models"
for f in kokoro-v1.0.onnx voices-v1.0.bin; do
  if [ ! -s "$OMATALK_HOME/models/$f" ]; then
    msg "Downloading $f (~340MB total) — this can take a few minutes depending on your connection"
    curl -L --fail --progress-bar -o "$OMATALK_HOME/models/$f" "$MODEL_BASE/$f"
  fi
done
msg "Verifying model checksums"
echo "${MODEL_SHA256}  $OMATALK_HOME/models/kokoro-v1.0.onnx" | sha256sum -c --quiet
echo "${VOICES_SHA256}  $OMATALK_HOME/models/voices-v1.0.bin" | sha256sum -c --quiet

# 5. systemd user service: warm daemon from login.
msg "Installing systemd user service"
mkdir -p "$HOME/.config/systemd/user" "$HOME/.local/bin"
cp "$OMATALK_HOME/src/systemd/omatalk.service" "$HOME/.config/systemd/user/"
systemctl --user daemon-reload
systemctl --user enable --now omatalk.service

# 6. Client on PATH.
cp "$OMATALK_HOME/venv/bin/omatalk" "$HOME/.local/bin/omatalk"

# 7. Keybindings are user-owned; the installer only prints the line.
msg "Add this line to ~/.config/hypr/bindings.lua (then reload):"
printf '  o.bind("F8", "Omatalk", "omatalk speak")\n'

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
