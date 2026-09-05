import re
from collections.abc import Iterator

# A long sentence, not Kokoro's 510-phoneme window. The onnxruntime arena
# grows with vocoder output length and never shrinks; 400 characters of
# dense text was enough to push RSS past 1GB. 160 is ~25 words / ~8–10s
# of audio. Short sentences pack into this window so a large blob is not
# one create() per period.
MAX_CHUNK = 160

_SENTENCE = re.compile(r"(?<=[.!?])(?:\s+|(?=[A-Z]))")
_CLAUSE = re.compile(r"[,;:]\s+")
_SPACE = re.compile(r"\s+")


def chunks(text: str) -> Iterator[str]:
    buf = ""
    for sentence in _SENTENCE.split(text.strip()):
        sentence = sentence.strip()
        if not sentence:
            continue
        if len(sentence) > MAX_CHUNK:
            if buf:
                yield buf
                buf = ""
            yield from _fit(sentence)
            continue
        candidate = f"{buf} {sentence}" if buf else sentence
        if len(candidate) > MAX_CHUNK:
            yield buf
            buf = sentence
        else:
            buf = candidate
    if buf:
        yield buf


def _fit(part: str) -> Iterator[str]:
    while part:
        if len(part) <= MAX_CHUNK:
            yield part
            return
        cut = _cut_at(part)
        head, rest = part[:cut].strip(), part[cut:].strip()
        if not head:
            head, rest = part[:MAX_CHUNK], part[MAX_CHUNK:].strip()
        yield head
        part = rest


def _cut_at(part: str) -> int:
    window = part[:MAX_CHUNK]
    last = None
    for match in _CLAUSE.finditer(window):
        last = match.end()
    if last:
        return last
    for match in _SPACE.finditer(window):
        last = match.end()
    if last:
        return last
    return MAX_CHUNK
