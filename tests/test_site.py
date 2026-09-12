from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INDEX = ROOT / "public" / "index.html"


class CodeCardParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.depth = 0
        self.card_depth = None
        self.in_pre = False
        self.pre_parts = []
        self.current = None
        self.cards = []

    def handle_starttag(self, tag, attrs):
        self.depth += 1
        classes = dict(attrs).get("class", "").split()
        if "code-card" in classes:
            self.card_depth = self.depth
            self.current = {"has_copy": False, "pre": ""}
        if self.current is not None and "copy-btn" in classes:
            self.current["has_copy"] = True
        if self.current is not None and tag == "pre":
            self.in_pre = True
            self.pre_parts = []

    def handle_endtag(self, tag):
        if self.in_pre and tag == "pre":
            self.in_pre = False
            if self.current is not None:
                self.current["pre"] = "".join(self.pre_parts)
        if self.card_depth is not None and self.depth == self.card_depth:
            self.cards.append(self.current)
            self.current = None
            self.card_depth = None
        self.depth -= 1

    def handle_data(self, data):
        if self.in_pre:
            self.pre_parts.append(data)


def _copy_cards():
    parser = CodeCardParser()
    parser.feed(INDEX.read_text())
    return [c for c in parser.cards if c["has_copy"]]


def test_copy_buttons_copy_the_visible_snippet():
    cards = _copy_cards()
    assert len(cards) == 5
    for card in cards:
        text = card["pre"].strip()
        assert text
        assert "grep -q" not in text
        assert ">>" not in text
        assert "hyprctl" not in text

    bindings = next(c for c in cards if "o.bind" in c["pre"])
    assert bindings["pre"].strip() == 'o.bind("F8", "Omatalk", "omatalk speak")'

    config = next(c for c in cards if "voice" in c["pre"] and "speed" in c["pre"])
    assert config["pre"].strip() == 'voice = "af_heart"\nspeed = 1.0'


def test_copy_handler_reads_the_visible_pre():
    html = INDEX.read_text()
    assert "pre.textContent" in html
    assert "dataset.copy" not in html
    assert "data-copy=" not in html
