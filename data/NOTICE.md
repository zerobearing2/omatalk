# Embedded data

These files are compiled into the binary and are version-locked to the
parity tests in `tools/parity/`. `build.rs` turns the lexicons into sorted
tables that are searched in place.

| File | Source | License |
| --- | --- | --- |
| `us_gold.json`, `us_silver.json` | misaki 0.9.4 lexicons (hexgrad/misaki) | Apache-2.0 |
| `tokenizer.json` | spaCy `en_core_web_sm` 3.8 tokenizer rules, exported by `tools/parity/export_tokenizer.py` | MIT |
| `tagger.bin` | spaCy `en_core_web_sm` 3.8 tok2vec and tagger weights, exported by `tools/parity/export_tagger.py` | MIT |
| `vocab.json` | Kokoro-82M v1.0 phoneme vocabulary (via kokoro-onnx) | Apache-2.0 / MIT |

## Updating misaki

1. Copy the new `us_gold.json` and `us_silver.json` into `data/`. The next
   build regenerates the tables; they are never committed.
2. Bump the `misaki[en]` pin in every script in `tools/parity/`, and the
   version in the table above.
3. Regenerate the reference corpora with `tools/parity/dump_misaki.py`.
4. Run `cargo test`, and `cargo test --release -- --ignored` with
   `OMATALK_MODELS` set.

A misaki release can change `en.py` along with the lexicons. The port pins
0.9.4's logic, so a parity failure after step 4 can mean an `en.py` change to
port, not a data problem. If the new release pins a different spaCy model,
re-export `tokenizer.json` and `tagger.bin` too.

`build.rs` accepts a phoneme string, or a tag map with a DEFAULT whose
values are phoneme strings or null. It stops the build on any other shape.
