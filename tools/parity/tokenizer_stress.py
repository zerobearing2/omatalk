#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12,<3.14"
# dependencies = [
#   "misaki[en]==0.9.4",
#   "spacy==3.8.16",
#   "en-core-web-sm @ https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl",
# ]
# ///
# Writes tools/parity/tokenizer-stress.jsonl: spaCy tokens for inputs the fiction
# corpus rarely exercises. Check with `cargo run --release --example parity -- tok tools/parity/tokenizer-stress.jsonl`.
import json, pathlib
from misaki import en

nlp = en.G2P(trf=False, fallback=None).nlp
cases = [
    "Visit https://example.com/path?q=1&r=2 or www.foo.org/bar, then email me@x.io.",
    "He said :) and :-( and <3 then ;) (:",
    "Don't can't won't shan't y'all ain't gonna 'tis 'em o'clock.",
    "The U.S.A. and U.K. met in Jan. with Dr. Smith, e.g., i.e. etc.",
    "It costs $4.99, US$5, £3 or 5€; 50% off! 10km at 3:30pm, 5'11\".",
    "C++ and C# and F# vs. node.js; a/b, a-b, a--b, a---b; 1-2, 3+4=7.",
    "Tabs\there,\n\nnewlines  double  spaces nbsp  thin.",
    "“Quoted,” he said—‘single’—and «guillemets»… wait...!?",
    "'90s rock'n'roll isn't dead; the '80s were.",
    "#hashtag @mention *bold* _under_ `code` ~tilde~ [link](http://a.b)",
    "Mr.Smith went to St.Louis. A.B.C. 1.2.3 v1.2.3 3.14 1,000,000.",
    "Emoji 😀 test, café naïve résumé Zürich Ωmega.",
    "(Nested (parens)) [brackets] {braces} <angle>",
    "Hello?!... Yes!!! No??? ——— ---",
    "10:30 a.m. and 2 p.m. and 5am, 6PM; 1st 2nd 3rd 4th.",
    "e-mail co-op re-enter self-driving well-known 20-year-old.",
    "IPv4 192.168.1.1 and 8.8.8.8:53 and foo@bar.com; x@y.",
    "   leading spaces and trailing   ",
    "we're they've I'd you'll she'd've Let's LET'S DON'T",
    "a.m.p.m. ok.) no.\" yes.' (e.g.) [i.e.]",
]
dest = pathlib.Path(__file__).resolve().parent / "tokenizer-stress.jsonl"
with dest.open("w") as f:
    for text in cases:
        doc = nlp(text.lstrip())
        f.write(json.dumps({"text": text, "tokens": [[t.text, t.tag_, t.whitespace_] for t in doc]}, ensure_ascii=False) + "\n")
print(dest, len(cases))
