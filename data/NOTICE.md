# Embedded data

These files are compiled into the binary (`include_bytes!`) and are
version-locked to the parity tests in `tools/parity/`.

| File | Source | License |
| --- | --- | --- |
| `us_gold.json`, `us_silver.json` | misaki 0.9.4 lexicons (hexgrad/misaki) | Apache-2.0 |
| `tokenizer.json` | spaCy `en_core_web_sm` 3.8 tokenizer rules, exported by `tools/parity/export_tokenizer.py` | MIT |
| `tagger.bin` | spaCy `en_core_web_sm` 3.8 tok2vec and tagger weights, exported by `tools/parity/export_tagger.py` | MIT |
| `vocab.json` | Kokoro-82M v1.0 phoneme vocabulary (via kokoro-onnx) | Apache-2.0 / MIT |
