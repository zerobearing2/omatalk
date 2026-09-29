#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12,<3.14"
# dependencies = [
#   "misaki[en]==0.9.4",
#   "spacy==3.8.16",
#   "en-core-web-sm @ https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl",
# ]
# ///
# Reference data for the Rust misaki port: spaCy tokens and tags as misaki
# sees them, plus misaki's phoneme output, one JSON object per line.
# Usage: tools/parity/dump_misaki.py <lines.txt> <out.jsonl>
import json, sys
from misaki import en, espeak
g2p = en.G2P(trf=False, british=False, fallback=espeak.EspeakFallback(british=False))
with open(sys.argv[2], "w") as out:
    for line in open(sys.argv[1]).read().splitlines():
        text = line.lstrip()
        if not text:
            continue
        tokens = [[t.text, t.tag, t.whitespace] for t in g2p.tokenize(text, [], {})]
        phonemes, _ = g2p(text)
        out.write(json.dumps({"text": text, "tokens": tokens, "phonemes": phonemes}, ensure_ascii=False) + "\n")
