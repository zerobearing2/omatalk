#!/usr/bin/env bash
# Omatalk installer: system deps (omarchy-approved), a pinned GitHub
# release tarball (the omatalk binary and its systemd user unit), models,
# and the bar plugin. Upgrades a v0.5 Python install in place. The bar
# plugin is `omarchy plugin add` of PLUGIN_REPO, not files from this
# tarball. scripts/build.sh rewrites RELEASE_TAG and TARBALL_SHA256.
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
# Fixed, not read from the environment: what this script downloads is
# exactly what it names here.
RELEASE_TAG="v0.9.0-dev.0"
TARBALL_SHA256="0000000000000000000000000000000000000000000000000000000000000000"
RELEASE_BASE="https://github.com/zerobearing2/omatalk/releases/download/${RELEASE_TAG}"
PLUGIN_REPO="https://github.com/zerobearing2/omarchy-omatalk-plugin.git"
MODEL_BASE="https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0"
MODEL_SHA256="7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5"
VOICES_SHA256="bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d"
TARBALL_MAX_BYTES=67108864
MODEL_MAX_BYTES=536870912
MODEL_FILE="kokoro-v1.0.onnx"
VOICES_FILE="voices-v1.0.bin"
LAUNCHER="$HOME/.local/bin/omatalk"

msg() {
  printf '\033[1;32m==>\033[0m %s\n' "$1"
}

warn() {
  printf '\033[1;33m==>\033[0m %s\n' "$1"
}

# Prompts read the terminal when piped via curl | bash; fall back to stdin
# for scripted runs where /dev/tty is unavailable. Tests set ASK_FROM.
if [ -z "${ASK_FROM:-}" ]; then
  ASK_FROM=/dev/tty
  if ! { : < /dev/tty; } 2>/dev/null; then
    ASK_FROM=/dev/stdin
  fi
fi

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

add_bar_plugin() {
  if omarchy plugin add "$PLUGIN_REPO" --enable --yes >/dev/null 2>&1; then
    return
  else
    warn "Could not add $PLUGIN_REPO; F8 still speaks. Add the plugin with: omarchy plugin add $PLUGIN_REPO --enable"
  fi
}

# 1. System dependencies. Omarchy only — omarchy pkg add, never pacman.
# The Daemon dlopens onnxruntime-cpu and espeak-ng; notify-send is libnotify.
if ! command -v omarchy >/dev/null 2>&1; then
  msg "Omatalk requires Omarchy. Install Omarchy, then rerun."
  exit 1
fi
PKG_DEPS=(curl pipewire wl-clipboard onnxruntime-cpu espeak-ng libnotify)
if omarchy pkg present "${PKG_DEPS[@]}"; then
  msg "System dependencies present: ${PKG_DEPS[*]}"
else
  msg "Installing missing packages via omarchy pkg add"
  omarchy pkg add "${PKG_DEPS[@]}"
fi

# 2. Pinned GitHub release tarball. Digest is in this script, not fetched
# beside the artifact. Unpacked into a staging directory; nothing installed
# is touched until the Daemon has stopped.
mkdir -p "$OMATALK_HOME"
staging="$(mktemp -d "$OMATALK_HOME/.release.XXXXXX")"
trap 'rm -rf "$staging" "${launcher_tmp:-}"' EXIT
msg "Downloading $RELEASE_TAG from GitHub"
download "$RELEASE_BASE/omatalk-x86_64.tar.gz" "$staging/omatalk-x86_64.tar.gz" "$TARBALL_MAX_BYTES"
echo "$TARBALL_SHA256  $staging/omatalk-x86_64.tar.gz" | sha256sum -c --quiet
tar -xzf "$staging/omatalk-x86_64.tar.gz" -C "$staging" --strip-components=1

# 3. Models (~355MB, skipped when their checksums match). Deliberately
# before the stop: the Daemon only reads models at startup, so fetching
# them while it still runs keeps its downtime to the binary + unit swap
# (and a failed download leaves the old Daemon untouched).
mkdir -p "$OMATALK_HOME/models"
download_model "$MODEL_FILE" "$MODEL_SHA256"
download_model "$VOICES_FILE" "$VOICES_SHA256"

# 4. Stop the Daemon before replacing the binary it runs from.
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

# 5. Binary and systemd user unit. The launcher is the binary itself, a
# regular file (the bar plugin probes it with test -f). Written beside it
# and renamed, so a reader never sees a half-written file and a running
# process keeps its old inode.
msg "Installing omatalk and its systemd user unit"
mkdir -p "$HOME/.config/systemd/user" "$HOME/.local/bin"
launcher_tmp="$(mktemp "$LAUNCHER.XXXXXX")"
install -m 0755 "$staging/omatalk" "$launcher_tmp"
mv -f "$launcher_tmp" "$LAUNCHER"
install -m 0644 "$staging/omatalk.service" "$HOME/.config/systemd/user/omatalk.service"

# 6. What a v0.5 Python install left behind. config.toml is never touched.
rm -rf "$OMATALK_HOME/venv" "$OMATALK_HOME/src"
rm -f "$HOME/.local/bin/omatalkd" \
  "$OMATALK_HOME/models/kokoro-v1.0.fp16.onnx" \
  "$OMATALK_HOME/omatalk-src.tar.gz"

systemctl --user daemon-reload
systemctl --user enable --now omatalk.service

# 7. F8 binding. Ask before writing bindings.lua; stay silent when an
# omatalk o.bind already exists (comments do not count, matching what
# uninstall removes); never take F8 from another command.
bindings="$HOME/.config/hypr/bindings.lua"
bind_line='o.bind("F8", "Omatalk", "omatalk speak")'
print_bind_command() {
  msg "To bind F8, paste this command (safe to re-run):"
  cat <<'EOF'
    grep -q omatalk ~/.config/hypr/bindings.lua || printf '\no.bind("F8", "Omatalk", "omatalk speak")\n' >> ~/.config/hypr/bindings.lua; hyprctl reload
EOF
}
already_bound=0
f8_taken=0
if [ -f "$bindings" ]; then
  if grep -qE 'o\.bind.*omatalk' "$bindings"; then
    already_bound=1
  else
    if grep -qF 'o.bind("F8"' "$bindings"; then
      f8_taken=1
    fi
  fi
fi
if [ "$already_bound" -eq 1 ]; then
  :
elif [ "$f8_taken" -eq 1 ]; then
  warn "F8 is already bound in $bindings; leaving it alone"
  print_bind_command
elif ! read -r -p "Bind F8 to omatalk speak in $bindings? [Y/n] " answer < "$ASK_FROM"; then
  print_bind_command
elif [[ "$answer" =~ ^[Nn] ]]; then
  print_bind_command
else
  mkdir -p "$(dirname "$bindings")"
  printf '\n%s\n' "$bind_line" >> "$bindings"
  msg "Bound F8 in $bindings"
  if ! hyprctl reload >/dev/null 2>&1; then
    warn "Could not reload Hyprland; run: hyprctl reload"
  fi
fi

# 8. Wait for the freshly installed daemon to answer status.
started=0
for _ in $(seq 1 30); do
  if "$LAUNCHER" status >/dev/null 2>&1; then
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

# Welcome speech proves the pipeline (service, socket, warm model, audio)
# end to end. The install is already complete, so a failure only warns.
if ! "$LAUNCHER" speak "Welcome to omatalk!" >/dev/null 2>&1; then
  warn "Welcome speech failed; check: journalctl --user -u omatalk"
fi

msg "Done. Select text and press F8, or run: omatalk speak|stop|status|upgrade|uninstall"
