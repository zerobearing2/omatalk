# Omatalk MVP Spec

*Settled via grilling session, 2026-08-31. Glossary: [CONTEXT.md](../CONTEXT.md). Decisions: [docs/adr/](./adr/).*

## What it is

Press a hotkey; the machine speaks the text you have selected, locally.
The reverse of dictation (voxtype): instead of your voice becoming text,
selected text becomes voice. Fully local — no network calls at runtime.

## Core loop

1. User highlights text anywhere in Hyprland.
2. User presses the Speak Key.
3. Daemon resolves the Source: **Selection** first (Wayland primary selection
   via `wl-paste --primary`), falling back to the **Clipboard** if the
   Selection is empty.
4. Daemon streams the text sentence-by-sentence through Kokoro-82M (ONNX,
   CPU) and plays audio via PipeWire (`pw-play`) as chunks are synthesized.
5. User presses the Speak Key again → **Interrupt**: if the Selection is
   unchanged or empty, speech stops; if the Selection changed, the current
   Utterance is cut off and the new one begins. (The Wayland primary
   selection is sticky after deselect, so unchanged text means silence.)

## Decisions (all confirmed)

| # | Decision | Choice |
|---|----------|--------|
| 1 | Audience | Community-grade from day one (omarchy ecosystem, voxtype as the shape-precedent) |
| 2 | Text source | Selection, with Clipboard fallback, one hotkey |
| 3 | Interaction | Single keypress: speak new Selection; while speaking, same-or-empty Selection = stop, changed Selection = interrupt + speak new (Wayland primary selection is sticky after deselect, so unchanged text means the user wants silence) |
| 4 | Interrupt semantics | Cut off immediately; speak new only when the Selection changed, otherwise stop |
| 5 | Engine | Kokoro-82M, ONNX Runtime, CPU (see [ADR-0001](./adr/0001-kokoro-onnx-cpu.md)) |
| 6 | Process shape | Persistent daemon, systemd user service, model warm from login |
| 7 | Language | Python daemon (uv-managed env, uv via `omarchy pkg add` if missing), shell glue; Rust is a later optimization (see [ADR-0002](./adr/0002-python-daemon.md)) |
| 8 | Feedback | notify-send for start/errors now; Quickshell widget designed-for, not built |
| 9 | Language/voice | English only; one configurable voice (`af_heart` default) + `speed` in config |
| 10 | Streaming | Sentence-by-sentence streaming, playback never waits for full synthesis |
| 11 | Hotkeys | `F8` (unmodified — adjacent to Omarchy's F9 dictation: F9 speaks you, F8 speaks back). `SUPER+CTRL+S` rejected: Omarchy binds it to Share. Via `~/.config/hypr/bindings.lua` |
| 12 | Name | Omatalk; daemon `omatalk`, config `~/.config/omatalk/`, state `~/.local/share/omatalk/` |
| 13 | License | MIT |
| 14 | Distribution | Curl-able install script in repo; AUR as fast-follow |
| 15 | Audio coexistence | Plays over other audio; media ducking is an opt-in config flag post-MVP |
| 16 | Daemon interface | Unix socket (`omatalk.sock`), line protocol: `speak <text>` / `stop` / `status` |
| 17 | Out of scope | Quickshell widget, voice picker UI, language detection, pause/resume, ducking, queueing, Piper fallback |

## Architecture

```
bindings.lua ──(F8)──▶ omarchy-spawned one-shot client
                                              │
                                              ▼
                                   Unix socket (omatalk.sock)
                                              │
                                              ▼
┌─────────────────────── omatalk daemon (systemd user service) ───────────────┐
│  capture: wl-paste --primary → wl-paste (clipboard fallback)                │
│  chunker: text → sentences                                                  │
│  engine:  Kokoro-82M ONNX (kokoro-onnx), warm session, CPU, espeak-ng phon. │
│  player:  stream PCM chunks → pw-play / PipeWire                            │
│  state:   idle | speaking | error  (reported over `status`)                 │
│  feedback: notify-send on errors (and optionally on start)                  │
└──────────────────────────────────────────────────────────────────────────────┘
```

Notes:

- The hotkey runs a **client** one-shot (`omatalk speak` / toggling handled by
  daemon). Client sends the request over the socket; daemon does the rest.
  Capture of the primary selection happens **in the daemon** (or client at
  request time) — it must resolve the Selection at press time, since the
  selection can change while speech is queued.
- Model + voice files (~300MB) download at install time into
  `~/.local/share/omatalk/models/`.
- Config: `~/.config/omatalk/config.toml` — `voice`, `speed` for MVP. Mirrors
  voxtype's `~/.config/voxtype/config.toml` convention.

## Error behavior

- Empty Source (nothing selected, clipboard empty): notification "Omatalk: nothing to read".
- Daemon not running: notification suggesting `systemctl --user start omatalk`.
- Synthesis/playback failure: notification with the error, daemon stays alive.

## Install (MVP, implemented in `install.sh`)

1. Check system deps; install missing ones via `omarchy pkg add` (python,
   git, curl, pipewire, wl-clipboard, uv). Stock Omarchy needs at most uv,
   and kokoro-onnx bundles its own phonemizer (espeakng-loader), so
   espeak-ng is not required.
2. Copy source to `~/.local/share/omatalk/src/`, create a uv venv at
   `~/.local/share/omatalk/venv/`, install the package (console scripts
   `omatalk` / `omatalkd`).
3. Download model + voice files (~340MB) from the kokoro-onnx v1.0 release
   into `~/.local/share/omatalk/models/` (skipped when present).
4. Install `systemd/omatalk.service` as a systemd user unit, enable + start.
5. Copy the client launcher to `~/.local/bin/omatalk`.
6. Print the `o.bind(...)` line for `~/.config/hypr/bindings.lua` (F8) —
   hotkeys are user-owned config, never touched by the installer.

## MVP acceptance criteria

1. Fresh omarchy machine: install script completes; after adding the
   binding and reloading, selecting text and pressing `F8` speaks
   it within ~1s (first audio).
2. Pressing the Speak Key mid-speech interrupts and speaks the new selection.
3. Works in Firefox, Chromium, a GTK app, and a terminal (primary selection);
   works in an Electron app via clipboard fallback after a copy.
4. `systemctl --user` survives relogin; daemon resident memory < ~1GB.
5. `omatalk status` reports state; no runtime network access.
