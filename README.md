# Omatalk

Local text-to-speech for Omarchy. Select text, press a hotkey, hear it read
back. The reverse of dictation: instead of you talking to the machine, it
talks to you. Fully local, no network calls at runtime.

## How it works

1. Highlight text anywhere in Hyprland.
2. Press `F8`, next to Omarchy's `F9` dictation key: F9 speaks you, F8 speaks
   back.
3. Omatalk reads the highlighted text, or the clipboard if nothing is
   selected. It streams the text sentence by sentence through
   [Kokoro-82M](https://huggingface.co/hexgrad/Kokoro-82M) (ONNX, CPU) and
   plays it over PipeWire as each sentence finishes synthesizing.
4. Press `F8` again while it's speaking to interrupt. If the selection hasn't
   changed, speech just stops. If it has, the old sentence cuts off and the
   new one starts.

On Omarchy, the bar shows a megaphone in the normal bar color and switches it to
the active color while Omatalk is speaking.

## Bar states

The bar keeps one megaphone icon in the same spot and changes its color:

- `idle`: normal bar color; the daemon is ready.
- `speaking`: theme accent; selected text is being read.
- `unavailable`: urgent color after a 3-second disconnect grace period.

The live bar context is softened around the Omatalk glyph so the state colors are easy to spot.

### Idle

The daemon is ready and the icon uses the normal bar color.

![Omatalk bar widget in its idle state](public/images/omatalk-bar-idle.png)

### Speaking

Omatalk is reading the current selection in the theme accent color.

![Omatalk bar widget in its speaking state](public/images/omatalk-bar-speaking.png)

### Unavailable

The daemon has been disconnected long enough to show the urgent color.

![Omatalk bar widget in its unavailable state](public/images/omatalk-bar-unavailable.png)

## Install

On an Omarchy machine:

```sh
curl -fsSL https://omatalk.zerobearing.com/install.sh | bash
```

The script downloads the latest release tarball (checksum-verified), then:

1. Checks system dependencies and installs any missing ones via
   `omarchy pkg add` (python, curl, pipewire, wl-clipboard, uv; stock
   Omarchy usually only lacks uv).
2. Builds a uv-managed venv at `~/.local/share/omatalk/venv/`.
3. Downloads the Kokoro-82M model and voice files (~185MB) to
   `~/.local/share/omatalk/models/`, skipped if already present.
4. Installs and enables the `omatalk.service` systemd user unit, so the
   daemon is warm from login and survives relogin.
5. Puts `omatalk` on `PATH`, installs the Omarchy bar plugin, and prints a
   copy-paste command that adds the F8 binding to `~/.config/hypr/bindings.lua`
   and reloads Hyprland. The installer never edits your keybindings itself.

Every push to `master` tags a new release automatically. Re-running the
installer picks up whatever is newest.

## Uninstall

```sh
curl -fsSL https://omatalk.zerobearing.com/uninstall.sh | bash
```

Stops and removes the systemd unit, the launcher, the source, and the
Omarchy bar plugin. Asks before deleting the models (~185MB) and your config.
Remove the F8 binding from `~/.config/hypr/bindings.lua` yourself.

## Usage

```sh
omatalk speak                # capture and speak (what the hotkey runs)
omatalk speak "text here"    # speak given text
omatalk stop                 # cut off the current utterance
omatalk status                # idle | speaking | error
```

`systemctl --user start|stop|restart omatalk` controls the daemon.
`journalctl --user -u omatalk -f` shows logs.

## Config

`~/.config/omatalk/config.toml`:

```toml
voice = "af_heart"
speed = 1.0
```

Restart the daemon after changing it (`systemctl --user restart omatalk`).
Voice previews are on the [project site](https://omatalk.zerobearing.com).

## Architecture

```
┌───────────────────┐   ┌─────────────────┐
│ bindings.lua (F8) │──▶│ one-shot client │
└───────────────────┘   └─────────────────┘
                                 │
                                 ▼
                    Unix socket (omatalk.sock)
                                 │
                                 ▼
┌───────────────────────────────────────────┐
│      omatalk daemon · systemd --user      │
│ capture:  wl-paste --primary → wl-paste   │
│ chunker:  text → sentences                │
│ engine:   Kokoro-82M · ONNX Runtime · CPU │
│ player:   pw-play → PipeWire              │
└───────────────────────────────────────────┘
```

The hotkey runs a one-shot client that sends a command over the socket; the
daemon does the rest. Its protocol verbs (`speak` / `stop` / `status`) and the
streaming `follow` command are the single seam: clients, the bar, tests, and
any future rewrite all go through it.

## Design docs

- [Domain language](CONTEXT.md)
- [ADRs](docs/adr/)

## License

MIT
