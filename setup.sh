#!/usr/bin/env bash
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
OMATALK_BIN="${OMATALK_BIN:-/usr/bin/omatalk}"
MODEL_BASE="https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0"
MODEL_SHA256="7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5"
VOICES_SHA256="bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d"
MODEL_MAX_BYTES=536870912
MODEL_FILE="kokoro-v1.0.onnx"
VOICES_FILE="voices-v1.0.bin"
PLUGIN_ID="zerobearing.omatalk"
PLUGIN_SOURCE="/usr/share/omatalk/omarchy-plugin/zerobearing.omatalk"
PLUGIN_DIR="$HOME/.config/omarchy/plugins/$PLUGIN_ID"
BINDINGS="$HOME/.config/hypr/bindings.lua"
CURL_UNIT="$HOME/.config/systemd/user/omatalk.service"
BIND_LINE='o.bind("F8", "Omatalk", "omatalk speak")'

msg() {
  printf '\033[1;32m==>\033[0m %s\n' "$1"
}

warn() {
  printf '\033[1;33m==>\033[0m %s\n' "$1"
}

if [ -z "${ASK_FROM:-}" ]; then
  ASK_FROM=/dev/tty
  if ! { : < /dev/tty; } 2>/dev/null; then
    ASK_FROM=/dev/stdin
  fi
fi

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
  local size="$3"
  local path="$OMATALK_HOME/models/$file"

  if [ -s "$path" ]; then
    if echo "$sha256  $path" | sha256sum -c --quiet; then
      return
    fi
  fi

  rm -f "$path"
  msg "Downloading $file (~$size) — this can take a few minutes depending on your connection"
  download "$MODEL_BASE/$file" "$path" "$MODEL_MAX_BYTES" --progress-bar
  echo "$sha256  $path" | sha256sum -c --quiet
}

step_models() {
  mkdir -p "$OMATALK_HOME/models"
  download_model "$MODEL_FILE" "$MODEL_SHA256" 326MB
  download_model "$VOICES_FILE" "$VOICES_SHA256" 28MB
}

step_enable() {
  if systemctl --user is-enabled --quiet omatalk.service; then
    return
  fi
  msg "Enabling omatalk.service"
  systemctl --user daemon-reload
  systemctl --user enable omatalk.service
}

# Curl installs copied the unit verbatim, so its ExecStart is one of two
# %h paths. `systemctl --user edit --full` writes a user's own override to
# the same file.
is_curl_era_unit() {
  local exec_start

  if ! exec_start=$(grep -m 1 '^ExecStart=' "$CURL_UNIT"); then
    return 1
  fi
  case "${exec_start#ExecStart=}" in
    "%h/.local/bin/omatalk"* | "$HOME/.local/bin/omatalk"*)
      return 0
      ;;
    "%h/.local/share/omatalk/venv/"* | "$HOME/.local/share/omatalk/venv/"*)
      return 0
      ;;
  esac
  return 1
}

stop_curl_era_unit() {
  local stop_status

  set +e
  systemctl --user disable --now omatalk.service 2>/dev/null
  stop_status=$?
  set -e
  if [ "$stop_status" -ne 0 ]; then
    if [ "$stop_status" -ne 5 ]; then
      msg "Could not stop the current daemon; refusing to remove its files"
      exit "$stop_status"
    fi
  fi
  msg "Removing the curl-era systemd unit"
  rm -f "$CURL_UNIT"
  systemctl --user daemon-reload
}

step_curl_era() {
  if [ -e "$CURL_UNIT" ]; then
    if is_curl_era_unit; then
      stop_curl_era_unit
    else
      warn "$CURL_UNIT is not a curl-era unit; leaving it. It overrides the packaged unit"
    fi
  fi

  rm -f "$HOME/.local/bin/omatalk" "$HOME/.local/bin/omatalkd"
  rm -rf "$OMATALK_HOME/venv" "$OMATALK_HOME/src"
  rm -f "$OMATALK_HOME/models/kokoro-v1.0.fp16.onnx" "$OMATALK_HOME/omatalk-src.tar.gz" \
    "$OMATALK_HOME/omatalk-src.tar.gz.sha256"
}

step_start_daemon() {
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

plugin_is_linked() {
  if [ -L "$PLUGIN_DIR" ]; then
    if [ "$(readlink "$PLUGIN_DIR")" = "$PLUGIN_SOURCE" ]; then
      return 0
    fi
  fi
  return 1
}

# What is at the path is a curl-era git checkout or copy, or a stale link.
# rm -rf removes a link without following it.
link_plugin() {
  if ! rm -rf "$PLUGIN_DIR"; then
    return 1
  fi
  if ! mkdir -p "$(dirname "$PLUGIN_DIR")"; then
    return 1
  fi
  ln -sfn "$PLUGIN_SOURCE" "$PLUGIN_DIR"
}

plugin_is_enabled() {
  omarchy plugin list --json 2>/dev/null | grep -Eq '"id":"zerobearing\.omatalk"[^}]*"enabled":true'
}

step_plugin() {
  if ! command -v omarchy >/dev/null 2>&1; then
    warn "omarchy not found; F8 still speaks. Without the bar plugin, rerun omatalk setup on Omarchy"
    return
  fi

  if plugin_is_linked; then
    return
  fi

  if [ ! -d "$PLUGIN_SOURCE" ]; then
    warn "$PLUGIN_SOURCE is missing; F8 still speaks. Reinstall the package: omarchy pkg drop omatalk-bin && omarchy pkg add omatalk-bin && omatalk setup"
    return
  fi

  local replaced=true
  if [ -L "$PLUGIN_DIR" ]; then
    warn "Replacing the link $PLUGIN_DIR -> $(readlink "$PLUGIN_DIR")"
  elif [ -d "$PLUGIN_DIR/.git" ]; then
    warn "Replacing the git checkout at $PLUGIN_DIR"
  elif [ -e "$PLUGIN_DIR" ]; then
    warn "Replacing $PLUGIN_DIR"
  else
    replaced=false
  fi

  msg "Linking the Omarchy bar plugin"
  if ! link_plugin; then
    warn "Could not link $PLUGIN_DIR; F8 still speaks. Link it with: ln -sfn $PLUGIN_SOURCE $PLUGIN_DIR"
    return
  fi

  if ! $replaced; then
    if ! omarchy-shell -q shell rescanPlugins; then
      warn "Could not rescan plugins; F8 still speaks. Reload the bar with: omarchy restart shell"
    fi
  fi
  if ! plugin_is_enabled; then
    if ! omarchy plugin enable "$PLUGIN_ID" >/dev/null 2>&1; then
      warn "Could not enable the bar plugin; F8 still speaks. Enable it with: omarchy plugin enable $PLUGIN_ID"
    fi
  fi
  # A rescan reloads a replaced plugin without reconnecting its widget; only a
  # shell restart brings it back.
  if $replaced; then
    if ! omarchy restart shell >/dev/null 2>&1; then
      warn "Could not restart the shell; F8 still speaks. Restart it with: omarchy restart shell"
    fi
  fi
}

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
step_curl_era
step_enable
step_start_daemon
step_binding
# The plugin goes before the wait, so a Daemon that fails to start never leaves
# the old curl-era checkout probing a deleted binary. The widget redials.
step_plugin
step_wait
step_welcome

msg "Done. Select text and press F8, or run: omatalk --help"
