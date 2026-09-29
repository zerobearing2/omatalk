# Language packs: en-gb now, more dialects without a recompile

Spec: https://github.com/zerobearing2/omatalk/issues/34

## Problem

The Rust Daemon speaks en-us only. British voices (`bf_*`, `bm_*`) get US phonemes, and a hand-edited `lang = "en-gb"` is read as en-us (`src/config.rs`, `load_reads_any_lang_as_en_us`). The lexicons are `include_bytes!` in the binary (about 9.8 MB), so adding a dialect today means a rebuild and a bigger binary.

Goal: en-gb works, and a dialect whose rules already exist ships as data that the Daemon loads at runtime, with no rebuild. Out of scope: ja, zh and any language that needs its own G2P code. The manifest names its pipeline so those can be added later without a format change.

## Decisions

Each was settled with the user in a grilling session. Rejected options are listed so nobody reopens them by accident.

1. **Scope.** English dialects only. A manifest field `pipeline = "en"` names the G2P pipeline. Other pipelines are a later spec.
2. **A pack is data.** A directory of lexicon tables and a manifest. No `.so` plugin (unstable interface, security surface, buys nothing for dialects). Not compiled in behind Cargo features (not switchable at runtime).
3. **Distribution.** en-us stays compiled in, so a first install works offline and current users see no change. Every other pack is downloaded from a release asset, sha256-pinned like the model, and installed under `$OMATALK_HOME/langs/<code>/`.
4. **Language follows the voice.** The voice prefix selects the pack (`bf_emma` selects en-gb). `lang` stops being a user-facing concept: a hand-edited `lang` keeps loading and is ignored, so v0.5 configs work. Two settings that can disagree is a bug factory.
5. **Applied per press.** The pack for the press's voice is loaded lazily and cached, the same as the per-press Config snapshot. A voice change already takes effect on the next press, and a language switch must not need a restart. The first press in a new language pays the load.
6. **Closed rule sets, open data.** The manifest names a `dialect` (`us` or `gb`) that selects a rule set written in Rust. Lexicon files and the espeak voice name are data. A dialect that needs new rules is a code change and a release. Rejected: granular per-rule flags in the manifest, because en-gb's differences are misaki's `british=True` branches, which are code.
7. **Pack layout.** `$OMATALK_HOME/langs/en-gb/manifest.toml`, `gold.lex`, `silver.lex`. The `.lex` format is byte-for-byte the one `build.rs` writes today, so one reader serves embedded and loaded tables. Rejected: a single archive file.
8. **Acquisition.** `omatalk lang add <code> | list | remove <code>`. It uses curl, checks the sha256 and installs by rename, like `upgrade`. It touches no native library, so the rule that the CLI path works with a broken system package holds. Rejected: the Daemon downloading on first use (network in the hotkey path), and an installer-only flag.
9. **Missing pack.** Speak with the US pack and warn once per Daemon run through the notify path. Failing the press would silence every current `bf_*` and `bm_*` user on upgrade. `config set voice bf_emma` succeeds and prints the same hint.
10. **Verification.** A British corpus from the pinned Python misaki (`british=True`), matched at the US bar: 499 of 500 rows, plus a held-out Gutenberg set of British text.
11. **Timing.** After 0.9.0 ships, as 0.10.0. Landing #30 and releasing 0.9.0 comes first.
12. **Data.** `data/gb_gold.json` and `data/gb_silver.json` (misaki 0.9.4, Apache-2.0) are committed like the US files, and `data/NOTICE.md` gets two rows. The pack builder stays offline and needs no Python.

Defaults baked in:

- Packs are read by `mmap` through `libc`, already a dependency. Resident memory stays lazy.
- The manifest carries `format = 1`. The Daemon refuses an unknown format with a clear error.
- The tokenizer, tagger, vocab and Kokoro model stay compiled in and are shared by every English pack.
- Moving en-us into the pack format is a later optional step, not in this spec.
- The espeak voice is switched on the synth thread between Utterances, never inside one.

## Facts the design rests on

Verified in the tree at `970f66a`.

- `build.rs:18-30` loops over `["us_gold","us_silver"]`, runs `grow` (en.py's `grow_dictionary`) and writes `$OUT_DIR/{name}.lex`. The format is documented at `build.rs:1-8`.
- `Lexicon::load()` (`src/speech/g2p/misaki/lexicon.rs:108`) is `include_bytes!` of the two tables. `Dict` (`lexicon.rs:14-76`) binary-searches a `&'static [u8]` in place. `Lexicon` has only `golds` and `silvers` fields, no language.
- `G2p::load()` (`src/speech/g2p/mod.rs`) takes no arguments. `Kokoro::load` (`src/speech/kokoro.rs:40-64`) calls it on a thread. Nothing in the engine, Daemon or protocol reads `lang`.
- `Espeak` hardcodes `set_voice(c"en-us")` (`src/speech/g2p/espeak.rs:79`). This machine's espeak-ng has `en-gb`, `en-gb-x-rp` and others.
- `VoiceName::parse` (`src/voices.rs:14-27`) splits a `lang` prefix and maps it to nothing. The local voices archive holds 54 voices across the prefixes `af am bf bm ef em ff hf hm if im jf jm pf pm zf zm`.
- US-specific rules with no British branch: `US_TAUS` in `misaki/phonemes.rs:11`, used at `lexicon.rs:339-340` (the `-ed` flap) and `lexicon.rs:377-379` (the `-ing` flap). The espeak-to-misaki table `E2M` and `fallback()` in `misaki/mod.rs:101-149` are the non-British path. The final `ɾ` to `T` and `ʔ` to `t` steps are at `misaki/mod.rs:423` and `number.rs:186`.
- Parity: `examples/parity.rs` (modes `tok|tag|e2e`), and `tools/parity/dump_misaki.py`, a uv script pinned to `misaki[en]==0.9.4` that hardcodes `british=False` (line 15). Corpora are US only. The `sample_corpus_matches_python_misaki` test needs libespeak-ng and is `#[ignore]`, with `SAMPLE_FLOOR` 499.
- Not audited: whether `numwords.rs` and `normalize.rs` have dialect-specific behavior. Ticket 05 checks this against the corpus.
- The tagger is spaCy `en_core_web_sm`. misaki's British path uses the same model, so the pack manifest says `tagger = "en_core_web_sm"` and the binary supplies it. Ticket 01 confirms this by diffing tags.

## Shape

```rust
/// A loaded language pack. Immutable, shared by the synth thread.
struct Pack { code: String, dialect: Dialect, espeak_voice: String, gold: Dict, silver: Dict }
enum Dialect { Us, Gb }

/// Prefix to pack code, built from installed manifests plus the embedded en-us.
struct Packs { by_prefix: HashMap<String, String>, loaded: HashMap<String, Arc<Pack>> }
impl Packs {
    fn pack_for(&mut self, voice: &VoiceName) -> Result<Arc<Pack>, PackError>; // lazy, cached
}
```

```toml
# manifest.toml
format = 1
code = "en-gb"
pipeline = "en"
dialect = "gb"
espeak_voice = "en-gb"
voice_prefixes = ["bf", "bm"]
tagger = "en_core_web_sm"
```

```sh
omatalk lang list          # "en-us  embedded" / "en-gb  installed  0.10.0"
omatalk lang add en-gb     # download, verify sha256, rename into place
omatalk lang remove en-gb
```

`G2p` takes a `&Pack` per call to `phonemize`, instead of owning one lexicon. The Kokoro engine asks `Packs` for the pack of the Utterance's voice and switches the espeak voice on the synth thread before running.

## Task order

The spec is issue #34. The tickets below become sub-issues of it. Each ends in a check the agent runs. Steps 02 and 03 prove the runtime-loading claim before any British rules exist.

1. `01-gb-parity-corpus`: `british=True` mode in `tools/parity/dump_misaki.py`, `gb-sample.jsonl`, held-out set. A failing test, no Rust change.
2. `02-pack-format-and-loader`: manifest parse, mmap, `Dict` over a mapped slice. Tested with a US pack built from the existing tables.
3. `03-pack-builder`: `examples/pack.rs` builds the US pack byte-identically to the embedded `.lex` files.
4. `04-language-selection`: prefix table, per-press pack choice, US fallback with one warning, espeak voice set per pack.
5. `05-gb-rules`: the `Gb` dialect branches until the corpus reaches 499 of 500.
6. `06-lang-cli`: `omatalk lang add|list|remove`. `install.md` and the README change in the same PR.
7. `07-release-asset`: publish `lang-en-gb-<ver>.tar.gz` plus its checksum from `scripts/`.

## Risks

- Rule drift. misaki's `british=True` touches more than lexicons. Ticket 05 is open-ended until the corpus passes, and it may find `numwords.rs` or `normalize.rs` differences.
- First-press latency. A first press in a new language pays the pack load and the espeak voice switch. Ticket 04 measures it, and a pre-warm on `config set voice` is the fallback if it is audible.
- The `sample_corpus` test needs libespeak-ng, so the British parity run is `#[ignore]` like the US one. CI must not depend on it.
- Pack and binary versions. `format = 1` guards the layout. If `Dialect` rules change between releases, a pack built for the old rules needs a `min_binary` field. Not built until a second format change forces it.
