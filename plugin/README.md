# Omatalk

Local text-to-speech for Omarchy: select text, press F8, the machine reads it
back. A megaphone in the bar follows idle, speaking, and unavailable.

![Omatalk config panel](preview.png)

## Requirements

- Omarchy with Quickshell plugin support.
- The Omatalk Daemon: uv, Kokoro models (~185MB), a systemd user unit,
  PipeWire, and wl-clipboard. If the Daemon is missing, the panel offers
  Install Omatalk.
- No sudo. No pkexec. The plugin does not start a second Quickshell process
  and does not parent the Daemon.

## Install

```sh
omarchy plugin add https://github.com/zerobearing2/omarchy-omatalk-plugin.git --enable
```

If the Daemon is not installed, click the megaphone and choose Install Omatalk.
That runs the public installer from https://omatalk.zerobearing.com in Omarchy's
floating terminal. Models are about 185MB.

The site is also a complete door for the Daemon, the launcher, and this plugin.

## Update

QML only:

```sh
omarchy plugin update zerobearing.omatalk --yes
```

Daemon, models, and unit:

```sh
omatalk upgrade
```

A Daemon upgrade does not rewrite a git-managed plugin checkout. A plugin
update does not rebuild the venv or stop the Daemon.

## Remove

```sh
omarchy plugin remove zerobearing.omatalk --yes
```

Plugin remove unloads the megaphone and deletes this checkout. It leaves the
Daemon, the venv, the models, and your config. F8 still speaks.

Full teardown (unit, launcher, plugin, optional models and config) is
`uninstall.sh` from the application repo / https://omatalk.zerobearing.com.

The installer never edits `bindings.lua` or `config.toml`.
