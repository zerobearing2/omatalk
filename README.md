# Omatalk

Local text-to-speech for Omarchy: press a hotkey and the machine speaks your
selected text. The reverse of dictation — instead of you talking to the
machine, the machine reads back to you. Fully local: no network calls at
runtime.

## How it works

1. Highlight text anywhere in Hyprland.
2. Press the Speak Key: plain `F8` — adjacent to the F9 dictation key
   (F9 speaks you, F8 speaks back). `SUPER+CTRL+S` was rejected: Omarchy
   binds it to Share.
3. The daemon resolves the **Source** at press time: the Selection (Wayland
   primary selection); if empty while idle, the Clipboard. It streams the
   text sentence-by-sentence through
   [Kokoro-82M](https://huggingface.co/hexgrad/Kokoro-82M) (ONNX, CPU) and
   plays audio via PipeWire as chunks are synthesized.
4. Press the Speak Key again while speaking → **Interrupt**:
   - Selection unchanged or empty → speech stops (the Wayland primary
     selection is sticky after you deselect, so unchanged text means silence).
   - Selection changed → the old utterance is cut off and the new one speaks.

## Install

On an Omarchy machine:

```sh
curl -fsSL https://omatalk.zerobearing.com/install.sh | bash
```

or from a clone of the repo: `./install.sh`.

The script installs, in order of preference: the **latest GitHub release
tarball** (checksum-verified, published by CI on every merge to `master`),
a git clone fallback (also used for updates when a release isn't
available — `OMATALK_SOURCE=1 ./install.sh` forces it), or the local
checkout you're running it from. Then it:

1. Checks system deps and installs any missing ones via `omarchy pkg add`
   (python, git, curl, pipewire, wl-clipboard, uv — stock Omarchy usually
   only lacks uv; kokoro-onnx bundles its own phonemizer, so no
   espeak-ng is needed).
2. Copies the source to `~/.local/share/omatalk/src/` and builds a uv-managed
   venv at `~/.local/share/omatalk/venv/`.
3. Downloads the Kokoro-82M model + voice files (~340MB) to
   `~/.local/share/omatalk/models/` (skipped if present).
4. Installs and enables the `omatalk.service` systemd user unit — the daemon
   is warm from login and survives relogin.
5. Puts `omatalk` on PATH (`~/.local/bin/`) and prints the `o.bind(...)`
   line for `~/.config/hypr/bindings.lua`. The installer never edits your
   keybindings.

Releases are built automatically: every push to `master` tags `v<version>`
(from `pyproject.toml`, patch auto-bumped when the tag already exists) and
attaches the source tarball + sha256 (see `.github/workflows/release.yml`).
The tag is the version record — the workflow never commits to the branch.
Re-running the installer picks up the newest release.

## Uninstall

```sh
./uninstall.sh
```

Stops and removes the systemd service, the launcher, and the source; asks
before deleting the models (~340MB) and your config. Remove the `o.bind`
line for F8 from `~/.config/hypr/bindings.lua` yourself.

## Usage

```sh
omatalk speak   # capture and speak (what the hotkey runs)
omatalk speak "text here"   # speak given text
omatalk stop    # cut off the current utterance
omatalk status  # idle | speaking | error
```

Daemon lifecycle is normal systemd: `systemctl --user start|stop|restart
omatalk`, `journalctl --user -u omatalk -f` for logs.

## Config

`~/.config/omatalk/config.toml`:

```toml
voice = "af_heart"
speed = 1.0
```

Restart the daemon after changing it (`systemctl --user restart omatalk`).
Voice preview samples live on the project site.

## Architecture

```
bindings.lua ──(F8)──▶ one-shot client (omatalk speak)
                                               │
                                               ▼
                                    Unix socket (omatalk.sock)
                                               │
                                               ▼
┌─────────────────────── omatalk daemon (systemd user service) ───────────────┐
│  capture: wl-paste --primary, clipboard fallback when idle                  │
│  chunker: text → sentences                                                  │
│  engine:  Kokoro-82M ONNX (kokoro-onnx), warm session, CPU                  │
│  player:  stream PCM chunks → pw-play / PipeWire                            │
└──────────────────────────────────────────────────────────────────────────────┘
```

State machine: `idle | speaking | error`. Interrupt semantics above. The
socket line protocol (`speak [text]` / `stop` / `status`) is the single
seam — the client, the tests, and any future rewrite all go through it.

## Design docs

- [MVP spec](docs/MVP.md)
- [Domain language](CONTEXT.md)
- [ADRs](docs/adr/)

## License

MIT
