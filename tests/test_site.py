from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlparse

from daemon.config import DEFAULTS

ROOT = Path(__file__).resolve().parent.parent
PUBLIC = ROOT / "public"
INDEX = PUBLIC / "index.html"
HOST = (PUBLIC / "CNAME").read_text().strip()

VOID = {
    "area",
    "base",
    "br",
    "col",
    "embed",
    "hr",
    "img",
    "input",
    "link",
    "meta",
    "param",
    "source",
    "track",
    "wbr",
}


class SiteParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.depth = 0
        self.card_depth = None
        self.in_pre = False
        self.pre_parts = []
        self.current_card = None
        self.cards = []
        self.voices = []
        self.voice = None
        self.in_voice_name = False
        self.in_voice_tag = False
        self.assets = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        classes = attrs.get("class", "").split()
        if tag not in VOID:
            self.depth += 1
        if "code-card" in classes:
            self.card_depth = self.depth
            self.current_card = {"has_copy": False, "pre": ""}
        if self.current_card is not None and "copy-btn" in classes:
            self.current_card["has_copy"] = True
        if self.current_card is not None and tag == "pre":
            self.in_pre = True
            self.pre_parts = []
        if tag == "button" and "voice-btn" in classes:
            self.voice = {
                "src": attrs.get("data-src", ""),
                "seed": "",
                "name": "",
                "default": False,
            }
        if self.voice is not None and "voice-avatar" in classes:
            self.voice["seed"] = attrs.get("data-seed", "")
        if self.voice is not None and "voice-name" in classes:
            self.in_voice_name = True
        if self.voice is not None and "voice-tag" in classes:
            self.in_voice_tag = True
        if tag == "img" and attrs.get("src"):
            self.assets.append(attrs["src"])
        if tag == "link" and "icon" in attrs.get("rel", "") and attrs.get("href"):
            self.assets.append(attrs["href"])
        if tag == "meta" and attrs.get("content"):
            prop = attrs.get("property", attrs.get("name", ""))
            if prop in {"og:image", "twitter:image"}:
                self.assets.append(attrs["content"])

    def handle_endtag(self, tag):
        if self.in_pre and tag == "pre":
            self.in_pre = False
            if self.current_card is not None:
                self.current_card["pre"] = "".join(self.pre_parts)
        if tag == "span":
            self.in_voice_name = False
            self.in_voice_tag = False
        if tag == "button" and self.voice is not None:
            self.voices.append(self.voice)
            self.voice = None
        if self.card_depth is not None and self.depth == self.card_depth:
            self.cards.append(self.current_card)
            self.current_card = None
            self.card_depth = None
        if tag not in VOID:
            self.depth -= 1

    def handle_data(self, data):
        if self.in_pre:
            self.pre_parts.append(data)
        if self.voice is not None and self.in_voice_name:
            self.voice["name"] += data
        if self.voice is not None and self.in_voice_tag and data.strip() == "default":
            self.voice["default"] = True


def parse_site():
    parser = SiteParser()
    parser.feed(INDEX.read_text())
    return parser


def copy_cards(site=None):
    site = site or parse_site()
    return [c for c in site.cards if c["has_copy"]]


def local_asset(url):
    parsed = urlparse(url)
    if parsed.scheme in {"http", "https"}:
        if parsed.hostname != HOST:
            return None
        rel = parsed.path.lstrip("/")
        return PUBLIC / rel if rel else None
    if url.startswith("#"):
        return None
    return PUBLIC / url.lstrip("/")


def test_copy_buttons_copy_the_visible_snippet():
    cards = copy_cards()
    assert len(cards) == 5
    for card in cards:
        text = card["pre"].strip()
        assert text
        assert "grep -q" not in text
        assert ">>" not in text
        assert "hyprctl" not in text

    bindings = next(c for c in cards if "o.bind" in c["pre"])
    assert bindings["pre"].strip() == 'o.bind("F8", "Omatalk", "omatalk speak")'


def test_copy_handler_reads_the_visible_pre():
    html = INDEX.read_text()
    assert "pre.textContent" in html
    assert "dataset.copy" not in html
    assert "data-copy=" not in html


def test_config_snippet_matches_daemon_defaults():
    expected = f'voice = "{DEFAULTS["voice"]}"\nspeed = {DEFAULTS["speed"]}'
    card = next(c for c in copy_cards() if "voice" in c["pre"] and "speed" in c["pre"])
    assert card["pre"].strip() == expected


def test_voice_buttons_match_clips_and_default():
    site = parse_site()
    assert site.voices
    page = {v["name"] for v in site.voices}
    disk = {p.stem for p in (PUBLIC / "audio" / "voices").glob("*.mp3")}
    assert page == disk, f"page-only {page - disk} disk-only {disk - page}"
    for voice in site.voices:
        assert voice["src"] == f"audio/voices/{voice['name']}.mp3"
        assert voice["seed"] == voice["name"]
        assert (PUBLIC / voice["src"]).is_file()
    defaults = [v["name"] for v in site.voices if v["default"]]
    assert defaults == [DEFAULTS["voice"]]


def test_install_commands_match_readme_and_cname():
    plugin = (
        "omarchy plugin add "
        "https://github.com/zerobearing2/omarchy-omatalk-plugin.git --enable"
    )
    install = f"curl -fsSL https://{HOST}/install.sh | bash"
    uninstall = f"curl -fsSL https://{HOST}/uninstall.sh | bash"
    shown = {c["pre"].strip() for c in copy_cards()}
    readme = (ROOT / "README.md").read_text()
    for command in (plugin, install, uninstall):
        assert command in shown
        assert command in readme
    assert (PUBLIC / "install.sh").is_file()
    assert (PUBLIC / "uninstall.sh").is_file()


def test_named_assets_exist():
    missing = []
    for url in parse_site().assets:
        path = local_asset(url)
        if path is None:
            continue
        if not path.is_file():
            missing.append(f"{url} -> {path}")
    assert missing == []
