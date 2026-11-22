#!/usr/bin/env bash
# Omatalk per-user setup, run by `omatalk setup` (embedded in the binary).
# The omatalk-bin package owns /usr/bin/omatalk and the systemd user unit.
# This does what pacman cannot do for one user: models, enabling the unit,
# the F8 binding, the bar plugin, and a welcome speech. Every step checks
# whether it is done already and skips itself, so a rerun changes nothing and
# asks nothing. The bar plugin is `omarchy plugin add` of PLUGIN_REPO.
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
OMATALK_BIN="${OMATALK_BIN:-/usr/bin/omatalk}"
# Fixed, not read from the environment: what this script downloads is
# exactly what it names here.
PLUGIN_REPO="https://github.com/zerobearing2/omarchy-omatalk-plugin.git"
MODEL_BASE="https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0"
MODEL_SHA256="7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5"
VOICES_SHA256="bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d"
MODEL_MAX_BYTES=536870912
MODEL_FILE="kokoro-v1.0.onnx"
VOICES_FILE="voices-v1.0.bin"
PLUGIN_ID="zerobearing.omatalk"
PLUGIN_DIR="$HOME/.config/omarchy/plugins/$PLUGIN_ID"
BINDINGS="$HOME/.config/hypr/bindings.lua"
BIND_LINE='o.bind("F8", "Omatalk", "omatalk speak")'

msg() {
  printf '\033[1;32m==>\033[0m %s\n' "$1"
}

warn() {
  printf '\033[1;33m==>\033[0m %s\n' "$1"
}

# Prompts read the terminal even when stdin is a pipe; fall back to stdin
# for scripted runs where /dev/tty is unavailable. Tests set ASK_FROM.
if [ -z "${ASK_FROM:-}" ]; then
  ASK_FROM=/dev/tty
  if ! { : < /dev/tty; } 2>/dev/null; then
    ASK_FROM=/dev/stdin
  fi
fi

# HTTPS only, including redirects (GitHub release assets redirect once to
# their CDN). Bounded in size and time; a stalled transfer aborts.
download() {
  local url="$1"
  local dest="$2"
  local max_bytes="$3"
  shift 3
  curl --fail \
    --proto '=https' \
    --proto-redir '=https' \
    --tlsv1.2 \
    --location \
    --max-redirs 5 \
    --max-filesize "$max_bytes" \
    --connect-timeout 30 \
    --speed-limit 1024 \
    --speed-time 60 \
    --max-time 3600 \
    "$@" \
    -o "$dest" \
    "$url"
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
  msg "Downloading $file (~355MB total) — this can take a few minutes depending on your connection"
  download "$MODEL_BASE/$file" "$path" "$MODEL_MAX_BYTES" --progress-bar
  echo "$sha256  $path" | sha256sum -c --quiet
}

# Done when both files exist with the pinned checksums. The Daemon reads
# them only at startup, so they come before the unit starts.
step_models() {
  mkdir -p "$OMATALK_HOME/models"
  download_model "$MODEL_FILE" "$MODEL_SHA256"
  download_model "$VOICES_FILE" "$VOICES_SHA256"
}

# Done when the unit is enabled. The package ships the unit but never
# enables it: that would start the Daemon before the models exist.
step_enable() {
  if systemctl --user is-enabled --quiet omatalk.service; then
    return
  fi
  msg "Enabling omatalk.service"
  systemctl --user daemon-reload
  systemctl --user enable omatalk.service
}

# Done when the Daemon answers status. restart, not start: a Daemon that
# is crash-looping without models picks them up at once.
step_daemon() {
  if "$OMATALK_BIN" status >/dev/null 2>&1; then
    return
  fi
  msg "Starting the daemon"
  systemctl --user restart omatalk.service
}

print_bind_command() {
  msg "To bind F8, paste this command (safe to re-run):"
  cat <<'EOF'
    grep -q omatalk ~/.config/hypr/bindings.lua || printf '\no.bind("F8", "Omatalk", "omatalk speak")\n' >> ~/.config/hypr/bindings.lua; hyprctl reload
EOF
}

# Done when bindings.lua has an omatalk o.bind (comments do not count,
# matching what uninstall removes). Otherwise ask before writing it, and
# never take F8 from another command.
step_binding() {
  local answer

  if [ -f "$BINDINGS" ]; then
    if grep -qE 'o\.bind.*omatalk' "$BINDINGS"; then
      return
    fi
    if grep -qF 'o.bind("F8"' "$BINDINGS"; then
      warn "F8 is already bound in $BINDINGS; leaving it alone"
      print_bind_command
      return
    fi
  fi

  if ! read -r -p "Bind F8 to omatalk speak in $BINDINGS? [Y/n] " answer < "$ASK_FROM"; then
    print_bind_command
    return
  fi
  if [[ "$answer" =~ ^[Nn] ]]; then
    print_bind_command
    return
  fi

  mkdir -p "$(dirname "$BINDINGS")"
  printf '\n%s\n' "$BIND_LINE" >> "$BINDINGS"
  msg "Bound F8 in $BINDINGS"
  if ! hyprctl reload >/dev/null 2>&1; then
    warn "Could not reload Hyprland; run: hyprctl reload"
  fi
}

# The Daemon must answer before the plugin and the welcome speech need it.
step_wait() {
  local attempt
  for attempt in $(seq 1 30); do
    if "$OMATALK_BIN" status >/dev/null 2>&1; then
      return
    fi
    sleep 1
  done
  msg "Daemon did not start; check: journalctl --user -u omatalk"
  exit 1
}

add_bar_plugin() {
  if omarchy plugin add "$PLUGIN_REPO" --enable --yes >/dev/null 2>&1; then
    return
  fi
  warn "Could not add $PLUGIN_REPO; F8 still speaks. Add the plugin with: omarchy plugin add $PLUGIN_REPO --enable"
}

# QML lives in PLUGIN_REPO. Official tools only: add when missing, update a
# git checkout (a user who has the 1.2.2 setup screen gets the packaged
# launcher path that way), and remove-then-add to convert a legacy copy into
# a git checkout. A failure only warns: the Daemon works without the plugin.
step_plugin() {
  if ! command -v omarchy >/dev/null 2>&1; then
    warn "omarchy not found; F8 still speaks. Add the plugin with: omarchy plugin add $PLUGIN_REPO --enable"
    return
  fi

  if [ -e "$PLUGIN_DIR/.git" ]; then
    msg "Updating the Omarchy bar plugin"
    if ! omarchy plugin update "$PLUGIN_ID" --yes >/dev/null 2>&1; then
      warn "Could not update the bar plugin; F8 still speaks. Update it with: omarchy plugin update $PLUGIN_ID --yes"
    fi
    return
  fi

  if [ -d "$PLUGIN_DIR" ]; then
    msg "Replacing copy-based Omarchy bar plugin with $PLUGIN_REPO"
    if omarchy plugin remove "$PLUGIN_ID" --yes >/dev/null 2>&1; then
      add_bar_plugin
    else
      warn "Could not remove the copy-based plugin; F8 still speaks. Convert it with: omarchy plugin remove $PLUGIN_ID --yes && omarchy plugin add $PLUGIN_REPO --enable"
    fi
    return
  fi

  msg "Installing Omarchy bar plugin"
  add_bar_plugin
}

# Proves the pipeline (service, socket, warm model, audio) end to end. The
# setup is already complete, so a failure only warns.
step_welcome() {
  if ! "$OMATALK_BIN" speak "Welcome to omatalk!" >/dev/null 2>&1; then
    warn "Welcome speech failed; check: journalctl --user -u omatalk"
  fi
}

if [ ! -x "$OMATALK_BIN" ]; then
  msg "$OMATALK_BIN is missing. Install the package first: omarchy pkg add omatalk-bin"
  exit 1
fi

step_models
step_enable
step_daemon
step_binding
step_wait
step_plugin
step_welcome

msg "Done. Select text and press F8, or run: omatalk --help"
