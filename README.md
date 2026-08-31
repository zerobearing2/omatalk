# Omatalk

Local text-to-speech for Omarchy: press a hotkey and the machine speaks your
selected text. The reverse of dictation — instead of you talking to the
machine, the machine reads back to you. Fully local: no network calls at
runtime.

## How it works

1. Highlight text anywhere in Hyprland.
2. Press the Speak Key (`SUPER+CTRL+S` primary, plain `F8` alternate —
   adjacent to the F9 dictation key).
3. The daemon captures the **Selection** (Wayland primary selection, with
   clipboard fallback), streams it sentence-by-sentence through
   [Kokoro-82M](https://huggingface.co/hexgrad/Kokoro-82M) (ONNX, CPU), and
   plays audio via PipeWire as chunks are synthesized.
4. Press the Speak Key again while speaking → **Interrupt**: the current
   utterance is cut off and the new press's text is spoken.

## Install (MVP)

Run the install script from the repo root:

```sh
./install.sh
```

It installs system deps (`espeak-ng`, PipeWire tools), sets up a uv-managed
Python environment with `kokoro-onnx`, downloads model + voice files
(~300MB) to `~/.local/share/omatalk/models/`, installs and starts the
`omatalk` systemd user service, and prints the two `o.bind(...)` lines to
add to `~/.config/hypr/bindings.lua`.

## Usage

The daemon runs from login; you interact via hotkeys or the one-shot client:

```sh
omatalk status   # idle | speaking | error
```

## Config

`~/.config/omatalk/config.toml`:

```toml
voice = "af_heart"
speed = 1.0
```

## Architecture

```
bindings.lua ──(SUPER+CTRL+S / F8)──▶ one-shot client
                                               │
                                               ▼
                                    Unix socket (omatalk.sock)
                                               │
                                               ▼
┌─────────────────────── omatalk daemon (systemd user service) ───────────────┐
│  capture: wl-paste --primary → wl-paste (clipboard fallback)                │
│  chunker: text → sentences                                                  │
│  engine:  Kokoro-82M ONNX (kokoro-onnx), warm session, CPU                  │
│  player:  stream PCM chunks → pw-play / PipeWire                            │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Design docs

- [MVP spec](docs/MVP.md)
- [Domain language](CONTEXT.md)
- [ADRs](docs/adr/)

## License

MIT
