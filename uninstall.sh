#!/usr/bin/env bash
# Omatalk uninstaller: stops and removes the service, launcher, source, and
# optionally models and config. Safe to run via curl | bash (prompts read
# the terminal, not the pipe).
set -euo pipefail

OMATALK_HOME="${OMATALK_HOME:-$HOME/.local/share/omatalk}"
UNIT="$HOME/.config/systemd/user/omatalk.service"

msg() { printf '\033[1;32m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$1"; }

# Prompts read the terminal when piped via curl | bash; fall back to stdin
# for scripted runs where /dev/tty is unavailable.
ASK_FROM=/dev/tty
{ : < /dev/tty; } 2>/dev/null || ASK_FROM=/dev/stdin

systemctl --user disable --now omatalk.service 2>/dev/null || true
rm -f "$UNIT"
systemctl --user daemon-reload
pkill -f "[o]matalk.daemon" 2>/dev/null || true
rm -rf "${XDG_RUNTIME_DIR:-/run/user/$UID}/omatalk"
rm -f "$HOME/.local/bin/omatalk" "$HOME/.local/bin/omatalkd"
if command -v omarchy >/dev/null 2>&1 && [ -d "$HOME/.config/omarchy/plugins/zerobearing.omatalk" ]; then
  if ! omarchy plugin remove zerobearing.omatalk --yes >/dev/null 2>&1; then
    shell_config="$HOME/.config/omarchy/shell.json"
    if [ -f "$shell_config" ]; then
      shell_config_tmp=$(mktemp)
      if jq --arg id "zerobearing.omatalk" '
        if (.bar.layout | type) == "object" then
          .bar.layout |= with_entries(.value |= map(select((.id // "") != $id)))
        else . end
        | if (.plugins | type) == "array" then
            .plugins |= map(select((.id // "") != $id))
          else . end
      ' "$shell_config" > "$shell_config_tmp"; then
        mv "$shell_config_tmp" "$shell_config"
      else
        rm -f "$shell_config_tmp"
        msg "Could not remove the Omatalk bar entry from $shell_config"
        exit 1
      fi
    fi
  fi
  rm -rf "$HOME/.config/omarchy/plugins"/.zerobearing.omatalk.bak.*
fi
rm -rf "$HOME/.config/omarchy/plugins/zerobearing.omatalk"
msg "Daemon stopped and removed; stray daemons killed; launcher and bar plugin removed"

if [ -d "$OMATALK_HOME" ]; then
  read -r -p "Remove $OMATALK_HOME (source, venv, ~340MB models)? [y/N] " answer < "$ASK_FROM"
  if [[ "$answer" =~ ^[Yy]$ ]]; then
    rm -rf "$OMATALK_HOME"
    msg "Removed $OMATALK_HOME"
  else
    warn "Kept $OMATALK_HOME"
  fi
fi

if [ -d "$HOME/.config/omatalk" ]; then
  read -r -p "Remove config $HOME/.config/omatalk? [y/N] " answer < "$ASK_FROM"
  if [[ "$answer" =~ ^[Yy]$ ]]; then
    rm -rf "$HOME/.config/omatalk"
    msg "Removed config"
  else
    warn "Kept config"
  fi
fi

msg "Omatalk uninstalled. Remove the o.bind line for F8 from ~/.config/hypr/bindings.lua."
