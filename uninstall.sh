#!/usr/bin/env bash
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
UNIT="$HOME/.config/systemd/user/omatalk.service"
PLUGIN_DIR="$HOME/.config/omarchy/plugins/zerobearing.omatalk"

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

# `omarchy pkg drop` below runs the package's remove hook, which stops and
# disables the packaged unit and removes the packaged plugin link. This script
# removes only what that hook cannot: curl-era files, models, config, and the
# F8 binding.

# Curl installs copied the unit verbatim, so its ExecStart is one of two %h
# paths. Any other file here is the user's own override.
is_curl_era_unit() {
  local exec_start

  if ! exec_start=$(grep -m 1 '^ExecStart=' "$UNIT"); then
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

if [ -e "$UNIT" ]; then
  if is_curl_era_unit; then
    set +e
    systemctl --user disable --now omatalk.service 2>/dev/null
    set -e
    rm -f "$UNIT"
    systemctl --user daemon-reload
  else
    warn "Kept $UNIT; it is your own unit, not a curl-era one"
  fi
fi
# Stray curl-era Daemons from v0.5 and 1.x. The regex dot in the first pattern
# matches the space in the 1.x argv. This script must never contain text
# either pattern matches: `omatalk uninstall` runs it as `bash -c <script>`,
# so its own argv holds that text and pkill -f would kill it.
if pgrep -f "[.]local/bin/omatalk.daemon" >/dev/null 2>&1; then
  set +e
  pkill -f "[.]local/bin/omatalk.daemon"
  set -e
fi
if pgrep -f "[d]aemon.omatalkd" >/dev/null 2>&1; then
  set +e
  pkill -f "[d]aemon.omatalkd"
  set -e
fi
rm -f "$HOME/.local/bin/omatalk" "$HOME/.local/bin/omatalkd"
rm -rf "$HOME/.config/omarchy/plugins"/.zerobearing.omatalk.bak.*
if [ -e "$PLUGIN_DIR" ]; then
  if [ ! -L "$PLUGIN_DIR" ]; then
    warn "Kept $PLUGIN_DIR; it is not the packaged plugin. Remove it with: rm -rf $PLUGIN_DIR"
  fi
fi

if [ -d "$OMATALK_HOME" ]; then
  if ! read -r -p "Remove $OMATALK_HOME (models, ~355MB)? [y/N] " answer < "$ASK_FROM"; then
    answer=""
  fi
  if [[ "$answer" =~ ^[Yy]$ ]]; then
    rm -rf "$OMATALK_HOME"
    msg "Removed $OMATALK_HOME"
  else
    warn "Kept $OMATALK_HOME"
  fi
fi

if [ -d "$HOME/.config/omatalk" ]; then
  if ! read -r -p "Remove config $HOME/.config/omatalk? [y/N] " answer < "$ASK_FROM"; then
    answer=""
  fi
  if [[ "$answer" =~ ^[Yy]$ ]]; then
    rm -rf "$HOME/.config/omatalk"
    msg "Removed config"
  else
    warn "Kept config"
  fi
fi

bindings="$HOME/.config/hypr/bindings.lua"
bind_note=""
if [ -f "$bindings" ]; then
  if grep -qE 'o\.bind.*omatalk' "$bindings"; then
    bind_note=" Remove the o.bind line for F8 from ~/.config/hypr/bindings.lua."
    if ! read -r -p "Remove the Omatalk binding from $bindings? [y/N] " answer < "$ASK_FROM"; then
      :
    elif [[ "$answer" =~ ^[Yy]$ ]]; then
      sed -i -E '/o\.bind.*omatalk/d' "$bindings"
      bind_note=" Removed the Omatalk binding."
      if ! hyprctl reload >/dev/null 2>&1; then
        warn "Could not reload Hyprland; run: hyprctl reload"
      fi
    fi
  fi
fi

msg "Omatalk files removed.$bind_note"

if ! command -v omarchy >/dev/null 2>&1; then
  warn "omarchy not found; remove the package with: sudo pacman -Rns omatalk-bin"
  exit 0
fi
if ! omarchy pkg present omatalk-bin; then
  msg "The omatalk-bin package is not installed; nothing to drop"
  exit 0
fi
msg "Dropping the omatalk-bin package"
exec omarchy pkg drop omatalk-bin
