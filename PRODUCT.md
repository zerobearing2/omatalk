# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Omarchy (Hyprland) users who live in the keyboard, many of whom already
dictate with a speech-to-text tool. They want long text read to them: a
changelog, a doc dropped in a channel, an email, a draft they wrote. They
arrive from the Omarchy community, the package list, or a link, and decide in
seconds whether this is worth one install command.

## Product Purpose

Omatalk reads selected text aloud. Select text, press F8, and it speaks,
using a neural voice that runs on the user's own CPU. Success on the site is a
visitor who understands the loop, hears a voice, and runs the install line.

## Positioning

The other half of voice on the desktop. Dictation tools such as voxtype,
Superwhisper, and Hex turn your voice into text; Omatalk turns text back into
voice. F9 dictates (voxtype's key), F8 reads back. They are companions, not
integrations: no claim that Omatalk talks to any of them.

What a neighbour cannot copy: fully local (no network calls at runtime, no
API key, no telemetry), a warm Daemon so speech starts about a third of a
second after the press and streams as it is synthesized, and it installs as
an Omarchy package and updates with `omarchy update`.

## Operating Context

- Install: `omarchy pkg add omatalk-bin && omatalk setup`. Setup downloads
  the models (about 355MB), enables the service, asks before binding F8, and
  links the bar plugin.
- Update: `omarchy update`. Uninstall: `omatalk uninstall`.
- Users who installed with the old curl installer run `omatalk upgrade` once;
  the site shows no curl command.
- Daily use: highlight anywhere, press F8; press again to interrupt. A
  megaphone in the Omarchy bar shows state and opens a settings panel.
- `omatalk render` turns a script into a WAV voiceover.

## Capabilities and Constraints

- Voice: Kokoro-82M via ONNX Runtime on CPU. 28 English voices (American and
  British). English only for now; more languages are a roadmap item.
- Config: `~/.config/omatalk/config.toml`, optional, read on every press.
- Render: about 10 s per minute of audio on a Ryzen 7840HS; peak memory about
  1 GiB.
- `tests/site.rs` pins the site's copy-button snippets (install, uninstall,
  config defaults, F8 binding, two render commands) against the README and the
  Daemon's defaults. Change them together.
- Terms follow `GLOSSARY.md` (Daemon, Selection, Utterance, Render, ...).

## Brand Commitments

- Name: Omatalk, lowercase `omatalk` as the wordmark. Built for Omarchy by
  DHH; credit Kokoro (hexgrad), kokoro-onnx (thewh1teagle), misaki.
- Existing site theme (JetBrains Mono display, IBM Plex text, teal/ember on
  light and dark) stays for this redesign by the owner's instruction; a theme
  refactor is planned later.
- Voice: plain, concrete, short sentences. No hype.

## Evidence on Hand

- Voice clips: `public/audio/voices/*.mp3`, one per voice, same line,
  rendered by the real pipeline.
- Screenshots: `public/images/omatalk-config-panel.png`, `og.png`.
- Measured: ~0.3 s to first audio; render timing above.
- No testimonials, user counts, or benchmarks against other TTS tools exist.
  Do not invent them.

## Product Principles

1. One command to install, one key to use. Everything else is optional.
2. Prove it by sound: let people hear the voices before they install.
3. Local is the point. Never imply a cloud, account, or network call.
4. Companion, not competitor, to dictation tools.
5. Depth lives in the README; the site links to it rather than repeating it.
