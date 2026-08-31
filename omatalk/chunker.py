import re

_SENTENCE = re.compile(r"(?<=[.!?])\s+")


def sentences(text: str) -> list[str]:
    parts = _SENTENCE.split(text.strip())
    return [p for p in parts if p.strip()]
