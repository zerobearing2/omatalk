#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12,<3.14"
# dependencies = [
#   "misaki[en]==0.9.4",
#   "spacy==3.8.16",
#   "en-core-web-sm @ https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl",
#   "numpy==2.5.2",
# ]
# ///
# Exports en_core_web_sm's tok2vec + tagger (the exact pipeline misaki runs)
# to data/tagger.bin for src/speech/g2p/tagger.rs, plus the string tables its features need.
# Usage: tools/parity/export_tagger.py <out.bin> [f32|f16|embed16|dense16]
import struct
import sys

import numpy as np
from misaki import en
from spacy.attrs import NORM, ORTH
from spacy.lang.norm_exceptions import BASE_NORMS
from spacy.strings import get_string_id
from spacy.symbols import IDS

nlp = en.G2P(trf=False, british=False).nlp
out = open(sys.argv[1], "wb")
dtype = sys.argv[2] if len(sys.argv) > 2 else "f32"


def u32(n):
    out.write(struct.pack("<I", n))


def string(s):
    b = s.encode("utf8")
    u32(len(b))
    out.write(b)


def floats(a, half=False):
    a = np.ascontiguousarray(a, dtype="<f2" if half else "<f4")
    out.write(a.tobytes())


out.write(b"TAG1")
embed_half = dtype in ("f16", "embed16")
dense_half = dtype in ("f16", "dense16")
u32(embed_half | dense_half << 1)

labels = nlp.get_pipe("tagger").labels
u32(len(labels))
for label in labels:
    string(label)

nodes = list(nlp.get_pipe("tok2vec").model.walk())
embeds = [n for n in nodes if n.name == "hashembed"]
u32(len(embeds))
for n in embeds:
    E = n.get_param("E")
    u32(E.shape[0])
    u32(n.attrs["seed"])
    floats(E, embed_half)

maxouts = [n for n in nodes if n.name == "maxout"]
norms = [n for n in nodes if n.name == "layernorm"]
assert len(maxouts) == len(norms) == 5
u32(len(maxouts))
for m, ln in zip(maxouts, norms):
    W = m.get_param("W")
    u32(W.shape[2])
    floats(W, dense_half)
    floats(m.get_param("b"))
    floats(ln.get_param("G"))
    floats(ln.get_param("b"))

softmax = nlp.get_pipe("tagger").model.get_ref("softmax")
floats(softmax.get_param("W"))
floats(softmax.get_param("b"))

symbols = [(k, v) for k, v in IDS.items() if k]
u32(len(symbols))
for k, v in symbols:
    string(k)
    u32(v)

# Vocab's NORM getter: lexeme_norm, then BASE_NORMS, then lower().
norms = {get_string_id(k): v for k, v in BASE_NORMS.items()}
norms.update(nlp.vocab.lookups.get_table("lexeme_norm").items())
u32(len(norms))
for key, value in norms.items():
    out.write(struct.pack("<Q", key))
    string(value)

# SHAPE uses Python's str.isalpha / isupper / isdigit: export them as
# code point ranges from this interpreter's Unicode database.
for test in (str.isalpha, str.isupper, str.isdigit):
    ranges, start = [], None
    for cp in range(0x110001):
        hit = cp <= 0x10FFFF and test(chr(cp))
        if hit and start is None:
            start = cp
        elif not hit and start is not None:
            ranges.append((start, cp - 1))
            start = None
    u32(len(ranges))
    for lo, hi in ranges:
        u32(lo)
        u32(hi)

rules = [(k, v) for k, v in nlp.tokenizer.rules.items() if any(NORM in p for p in v)]
u32(len(rules))
for _, pieces in rules:
    u32(len(pieces))
    for p in pieces:
        string(p[ORTH])
        string(p.get(NORM, ""))
out.close()
