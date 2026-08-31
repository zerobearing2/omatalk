# Python daemon instead of Rust, with socket protocol as the seam

Voxtype — the tool Omatalk mirrors — is Rust, and the omarchy ecosystem
naturally expects small system daemons to be Rust single binaries. We chose a
**Python daemon (uv-managed, kokoro-onnx)** anyway. The MVP's risk is
integration (Wayland selection capture, streaming synthesis, PipeWire
playback, socket protocol), not performance: Kokoro-82M on CPU is far faster
than real-time, and the phonemization stack (misaki/espeak-ng) is
Python-native.

We revisited this decision with real experiments before writing any code, and
it held. Full numbers live in `.scratch/benchmark/issues/01-kokoro-cpu-benchmark.md`;
summary of the alternatives tested and rejected:

- **speech-dispatcher** — would make the backend swappable at the system
  level, but no Kokoro output module exists, so it caps us at espeak/Piper
  quality (the bar ADR-0001 rejected), adds a daemon-of-a-daemon whose
  queueing/priority semantics fight Omatalk's interrupt design, and sits
  between synthesis and playback where streaming latency is controlled.
- **Kokoro-in-Rust crates** (kokoro-tiny, kokoro-rs/kokoro-cli) — all fail at
  the same point: none faithfully reproduces the Python
  phonemization→tokenization pipeline, and that pipeline decides Kokoro's
  quality. Concretely: kokoro-tiny emitted a WAV that is 55% near-silence
  (reference: 7%) with 22% duration compression; kokoro-cli produced
  clipping-level garbage after three manual repairs (link deps, model swap,
  token-table swap) and crashes on long input. Meanwhile the performance
  argument for Rust was always weak — the neural net runs in the same C++
  onnxruntime under both languages, and the model load (~0.6-0.8s), the only
  per-press cost, is eliminated by the warm daemon in either language.
- **The Python reference pipeline** (kokoro-onnx, v1.0 model, system
  espeak-ng) sounded correct on the first try (RTF ~0.16, ~6x faster than
  real-time) and is what ships.

Consequences: the install script ships a uv-managed environment rather than a
single binary; resident memory is Python-interpreter-sized (~300-500MB total)
instead of tens of MB. The daemon's socket protocol is the seam: a Rust
rewrite replaces the process, not the interface. The bar for reopening the
Rust question is evidence-based: a Rust pipeline must beat the Python
reference in a blind listening test and waveform analysis — not in a README.
