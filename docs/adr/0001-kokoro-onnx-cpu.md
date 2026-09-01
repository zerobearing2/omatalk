# Kokoro-82M via ONNX Runtime on CPU as the engine

Omatalk's entire value is natural-sounding local speech on a hotkey, so engine
quality is the product. We benchmarked the 2026 local TTS field: Piper
(RTF 0.008, ~40ms latency, but audibly robotic), Chatterbox and XTTS v2
(higher ceiling but GPU-sized, and XTTS is non-commercial), and Kokoro-82M.
We chose **Kokoro-82M, ONNX Runtime, CPU**: near top-of-class naturalness
(MOS ~4.2), ~90ms first-audio and faster-than-real-time even on the target
hardware (Ryzen 7840HS-class), streams sentence chunks, Apache 2.0, ~300MB.
Piper's speed advantage is irrelevant on any modern CPU; Kokoro's quality gap
is the whole difference between "fun demo" and "daily driver".

Considered and rejected:

- **Piper** — fallback candidate only; quality below the bar for a
  read-aloud tool. Revisit as a low-power/battery profile, not MVP.
- **Chatterbox (MIT)** — best cloning/expressiveness but 0.5B and
  GPU-preferred; no cloning need in MVP.
- **XTTS v2 / F5-TTS** — voice cloning we don't need; XTTS is CPML
  (non-commercial), F5 weights CC-BY-NC.

Consequences: no voice cloning is possible on this engine (fixed voice
packs); English-first with 8-9 language voices available. espeak-ng is a hard
phonemizer dependency.

Revision 2026-08-31: the shipped artifact is the **fp16 export**
(`kokoro-v1.0.fp16.onnx`, ~185MB download with voices), not the fp32 file
this ADR originally sized at ~300MB. Validated by spectral correlation 0.999
against fp32 plus a live listening test, not a blind comparison. Resident
memory measured a wash versus fp32 (the CPU provider upcasts at load);
kokoro-onnx 0.6 is required — the v1.1 exports declare `speed` as float and
fail on 0.4.x.
