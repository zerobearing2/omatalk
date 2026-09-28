# Rust Daemon: one binary, misaki G2P, fp32 through system onnxruntime

Supersedes ADR-0002.

ADR-0002 kept a Python Daemon until a Rust pipeline could match the Python
reference in a listening test. A prototype
(`.scratch/rust-binary/`, findings in `issues/01-findings.md`) did that, so
Omatalk 0.9 replaces the Python Daemon and CLI with one Rust binary.
`omatalk daemon` is the Daemon. Every other argv is the CLI. The socket
protocol, CLI output, config file, unit name, and launcher path do not change,
so the bar plugin and the F8 binding do not notice.

The prototype measured where the time goes. Python was not the latency cost.
The ONNX graph was, at about 155 ms per call plus about 6 ms per phoneme.
Three changes follow from that:

- **Short first batch.** v0.5.1 packed up to 160 characters before the first
  synthesis, so a paragraph waited more than 1.3 s for first audio. The first
  batch is now about 32 phonemes, and later batches grow up to Kokoro's
  510-phoneme limit. Expected first audio is 0.25 to 0.4 s.
- **fp32 model.** The fp32 v1.0 export (`kokoro-v1.0.onnx`, 310 MB, release
  `model-files-v1.0` of thewh1teagle/kokoro-onnx) is about 35% faster than
  the fp16 export v0.5 shipped and loads twice as fast. On a Ryzen 7840HS,
  fp16 took 278 ms to first audio for "Hello." against 176 ms for fp32, and
  693 ms to load against 357 ms. The CPU has no native fp16, so onnxruntime
  upcasts on every run. int8 is 5x slower. We accept the larger download
  (about 355 MB with voices, against 185 MB) for faster speech on every press.
- **misaki G2P, ported.** misaki is Kokoro's own grapheme-to-phoneme library.
  The port covers its normalizer, the spaCy `en_core_web_sm` 3.8 tokenizer and
  tagger, and the misaki lexicons. It matches Python misaki on 28,758 of
  28,759 corpus lines. It reads heteronyms correctly (wound, dove, read),
  and it sounded better than espeak in a listening test. espeak-ng remains
  only as misaki's fallback for words outside the lexicon.

The binary links onnxruntime dynamically (`ort` with `load-dynamic`) against
Arch's `onnxruntime-cpu`, and it dlopens espeak-ng. Only `omatalk daemon`
loads them, so `omatalk version` and `omatalk config` work even if a system
package breaks. Against onnxruntime 1.29, output is bit-identical to Python.
The binary is about 19 MB, including 14.5 MB of embedded G2P data.

Considered and rejected:

- **Keep Python, fix the batching.** The first-audio fix works in either
  language. The Python stack still pulled a uv-managed venv, a hashed
  requirements file, and an interpreter-sized process, and it could not ship
  as one AUR package.
- **Two binaries (`omatalk` and `omatalkd`).** This doubles the install and
  packaging artifacts for no separation that `omatalk daemon` lacks.
- **Bundled onnxruntime (static link).** A 31 MB binary against 4.8 MB, and a
  different onnxruntime version than the one parity was measured against.

Consequences: the installer ships one binary and a unit instead of a venv.
System dependencies change from `python` and `uv` to `onnxruntime-cpu` and
`espeak-ng`. The installer migrates a v0.5 install in place and deletes its
venv, source, and fp16 model. Resident memory stays in the 450 to 750 MB range
because the onnxruntime arena dominates it in either language. The idle
self-recycle from ADR-0002 is replaced by an idle rest. Every run is capped at
510 phonemes, which bounds the arena at about 1.3 GB, and a minute after the
last synthesis the synth thread runs one small inference that shrinks it back
to about 510 MB, with no restart. The
design and its measurements live in `docs/design/rust-rewrite.md`.
