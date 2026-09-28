#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12,<3.14"
# dependencies = [
#   "misaki[en]==0.9.4",
#   "spacy==3.8.16",
#   "en-core-web-sm @ https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl",
# ]
# ///
# Dumps en_core_web_sm's tokenizer config to data/tokenizer.json for src/speech/g2p/tokenize.rs.
import json, pathlib, spacy
from misaki import en

# misaki installs en_core_web_sm on first G2P construction.
tok = en.G2P(trf=False, fallback=None).nlp.tokenizer

def pat(f):
    return None if f is None else f.__self__.pattern

def alternatives(pattern):
    # Top-level `|` split, so the Rust side can route lookaround-free
    # alternatives to the regex crate's automata.
    alts, depth, cls, start, i = [], 0, False, 0, 0
    while i < len(pattern):
        c = pattern[i]
        if c == "\\":
            i += 1
        elif cls:
            cls = c != "]"
        elif c == "[":
            cls = True
            if pattern[i + 1 : i + 2] == "]":
                i += 1
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
        elif c == "|" and depth == 0:
            alts.append(pattern[start:i])
            start = i + 1
        i += 1
    alts.append(pattern[start:])
    assert "|".join(alts) == pattern and depth == 0
    return alts


def flags(f):
    return None if f is None else f.__self__.flags

out = {
    "spacy": spacy.__version__,
    "prefix": alternatives(pat(tok.prefix_search)),
    "suffix": alternatives(pat(tok.suffix_search)),
    "infix": pat(tok.infix_finditer),
    "token_match": pat(tok.token_match),
    "url_match": pat(tok.url_match),
    "flags": {k: flags(getattr(tok, k)) for k in ["prefix_search", "suffix_search", "infix_finditer", "token_match", "url_match"]},
    "faster_heuristics": tok.faster_heuristics,
    "rules": {k: [t.get(65, t.get("ORTH")) for t in v] for k, v in sorted(tok.rules.items())},
}
dest = pathlib.Path(__file__).resolve().parents[2] / "data" / "tokenizer.json"
dest.write_text(json.dumps(out, ensure_ascii=False, indent=0))
print(dest, len(out["rules"]), "rules")
