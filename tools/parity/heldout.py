#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12,<3.14"
# dependencies = []
# ///
# Builds a held-out tagger check set (not in misaki.jsonl): Gutenberg books
# from <train dir> (pg*.txt), this repo's docs and code comments, and
# stress.txt. Feed the output to dump_misaki.py for a parity corpus.
# Usage: tools/parity/heldout.py <train dir> <out.txt>
import pathlib, re, sys

root = pathlib.Path(__file__).resolve().parents[2]
train = pathlib.Path(sys.argv[1])
lines = []
for book in sorted(train.glob("pg*.txt")):
    text = book.read_text(encoding="utf-8-sig")
    body = text.split("*** START")[1].split("*** END")[0].split("\n", 1)[1]
    paras = [" ".join(p.split()) for p in re.split(r"\n\s*\n", body)]
    lines += [p for p in paras if len(p) > 20][:1500]
for doc in sorted(root.glob("*.md")) + sorted(root.glob("docs/**/*.md")):
    lines += [" ".join(p.split()) for p in re.split(r"\n\s*\n", doc.read_text()) if p.strip()]
for src in sorted(root.glob("src/**/*.rs")):
    for m in re.finditer(r"(?:#|//) ?(.+)", src.read_text()):
        if len(m.group(1)) > 20:
            lines.append(m.group(1).strip())
lines += pathlib.Path(__file__).with_name("stress.txt").read_text().splitlines()
pathlib.Path(sys.argv[2]).write_text("\n".join(lines) + "\n")
print(len(lines), "lines")
