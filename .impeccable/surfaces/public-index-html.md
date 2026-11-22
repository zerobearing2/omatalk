---
version: 1
slug: "public-index-html"
primary_target: "public/index.html"
related_targets: []
---

# Surface: public/index.html (site home)

Mode: Persuade. Audience: Omarchy users, many already dictating with voxtype,
superwhisper or Hex. Action: run `omarchy pkg add omatalk-bin && omatalk setup`.
Proof: the real voice clips, played on the page. Constraints: existing theme
(tokens, fonts, light/dark) kept by owner instruction; no curl on the page;
copy-card snippets pinned by tests/site.rs; depth (bar states, panel, config
table, troubleshooting) lives in README.

## Direction contract

THESIS: The F8 key is the page. A visitor presses a real keycap and hears the
product, instead of reading a hero over a terminal mock and eleven sections.

OWN-WORLD: The incumbent world: tinted paper/charcoal grounds, JetBrains Mono
display, IBM Plex text, teal (light) / ember (dark) accent, hairline rules,
code cards with copy buttons. Keycaps are the one new component: bordered,
soft-offset shadow, press travel.

STORY: Understand the loop (select, F8, hear it); believe it is fast and local
by hearing it and reading four plain facts; see it as the other half of their
dictation tool; copy one install line.

FIRST VIEWPORT: Headline "Select text. / Press [F8]. / Hear it." with F8 as a
static display-scale keycap (not interactive: F8 belongs to a visitor's own
install). Under it, one paragraph with a listen button that plays it whole in
af_heart, rendered by `omatalk render`. No word-by-word highlighting: that is
the roadmap's on-screen text feature, not shipped. Install card follows
(above the paragraph on mobile).

FORM: Key-led single column, position 5 of 7 on the ranked list. seed:d7951263d9b44054

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
