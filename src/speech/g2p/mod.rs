//! Text to Kokoro phonemes: normalize, spaCy tokenizer port, spaCy tagger
//! port, misaki lexicon port, espeak-ng for out-of-vocabulary words.
//! Parity levers: `examples/parity.rs`.

pub mod espeak;
pub mod misaki;
pub mod normalize;
mod numwords;
pub mod tagger;
pub mod tokenize;

use std::sync::Mutex;

use super::LoadError;

/// Immutable after load; espeak-ng's C state sits behind its own mutex.
pub struct G2p {
    tokenizer: tokenize::Tokenizer,
    tagger: tagger::Tagger,
    lexicon: misaki::Lexicon,
    espeak: &'static Mutex<espeak::Espeak>,
}

impl G2p {
    /// Parses the embedded lexicons (~110 ms), tokenizer rules, and tagger
    /// (~5 ms); dlopens libespeak-ng. Called once, on a loader thread.
    pub fn load() -> Result<G2p, LoadError> {
        Ok(G2p {
            tokenizer: tokenize::Tokenizer::load()?,
            tagger: tagger::Tagger::load()?,
            lexicon: misaki::Lexicon::load()?,
            espeak: espeak::espeak()?,
        })
    }

    /// One line of text to phonemes, `""` for a blank line. Normalizes per
    /// line: fancy-regex's backtrack limit trips on huge input.
    pub fn line(&self, text: &str) -> String {
        self.phonemize(&normalize::normalize(text), "")
    }

    /// Python misaki's `G2P(text)` on already-normalized text, with `unk` for
    /// words neither the lexicon nor espeak can read. The parity target.
    pub fn phonemize(&self, text: &str, unk: &str) -> String {
        let text = text.trim_start_matches(tokenize::is_space);
        if text.is_empty() {
            return String::new();
        }
        let pieces = self.tokenizer.tokenize(text);
        let words: Vec<&str> = pieces.iter().map(|(w, _)| w.as_str()).collect();
        let spaces: Vec<bool> = pieces.iter().map(|(_, ws)| !ws.is_empty()).collect();
        let tags = self.tagger.tag(&words, &spaces);
        let tokens = pieces
            .iter()
            .zip(&tags)
            .map(|((w, ws), tag)| misaki::Tok::new(w, tag, ws))
            .collect();
        let espeak = self.espeak.lock().unwrap_or_else(|e| e.into_inner());
        misaki::g2p(&self.lexicon, tokens, unk, |text| {
            espeak.phonemize_tied(text)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{G2p, misaki, tagger::Tagger, tokenize, tokenize::Tokenizer};

    const SAMPLE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tools/parity/sample.jsonl"
    ));
    const TOKENIZER_STRESS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tools/parity/tokenizer-stress.jsonl"
    ));

    /// A `tools/parity/dump_misaki.py` row: spaCy's (text, tag, whitespace)
    /// per token, and Python misaki's phonemes.
    struct Row {
        text: String,
        tokens: Vec<(String, String, String)>,
        phonemes: String,
    }

    fn rows(corpus: &str) -> Vec<Row> {
        let s = |v: &serde_json::Value| v.as_str().unwrap_or_default().to_owned();
        corpus
            .lines()
            .map(|line| {
                let v: serde_json::Value = serde_json::from_str(line).unwrap();
                Row {
                    text: s(&v["text"]),
                    tokens: v["tokens"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| (s(&t[0]), s(&t[1]), s(&t[2])))
                        .collect(),
                    phonemes: s(&v["phonemes"]),
                }
            })
            .collect()
    }

    #[test]
    fn tokenizer_matches_spacy() {
        let tokenizer = Tokenizer::load().unwrap();
        for row in rows(SAMPLE).into_iter().chain(rows(TOKENIZER_STRESS)) {
            let want: Vec<(String, String)> =
                row.tokens.into_iter().map(|(t, _, ws)| (t, ws)).collect();
            assert_eq!(
                tokenizer.tokenize(row.text.trim_start_matches(tokenize::is_space)),
                want,
                "{:?}",
                row.text
            );
        }
    }

    /// Unoptimized, the tagger takes ~3 ms a token, so a debug build tags
    /// every 50th row (~1 s) and a release build all of them.
    #[test]
    fn tagger_matches_spacy() {
        let tagger = Tagger::load().unwrap();
        let step = if cfg!(debug_assertions) { 50 } else { 1 };
        for row in rows(SAMPLE).into_iter().step_by(step) {
            let words: Vec<&str> = row.tokens.iter().map(|t| t.0.as_str()).collect();
            let spaces: Vec<bool> = row.tokens.iter().map(|t| !t.2.is_empty()).collect();
            let want: Vec<&str> = row.tokens.iter().map(|t| t.1.as_str()).collect();
            assert_eq!(tagger.tag(&words, &spaces), want, "{:?}", row.text);
        }
    }

    /// Rows that never reach espeak, so misaki alone must match Python.
    #[test]
    fn misaki_matches_python_without_espeak() {
        let lexicon = misaki::Lexicon::load().unwrap();
        let mut checked = 0;
        for row in rows(SAMPLE) {
            let tokens = row
                .tokens
                .iter()
                .map(|(w, tag, ws)| misaki::Tok::new(w, tag, ws))
                .collect();
            let mut espeak = false;
            let got = misaki::g2p(&lexicon, tokens, "❓", |_| {
                espeak = true;
                String::new()
            });
            if !espeak {
                assert_eq!(got, row.phonemes, "{:?}", row.text);
                checked += 1;
            }
        }
        assert_eq!(checked, 300, "rows that never reach espeak");
    }

    /// Rows of `tools/parity/sample.jsonl` (500) that match Python misaki. The
    /// miss is misaki's `[text](feature)` markdown syntax, skipped on purpose:
    /// on selected text it would drop the parenthetical.
    const SAMPLE_FLOOR: usize = 499;

    #[test]
    #[ignore = "needs libespeak-ng; run with --ignored"]
    fn sample_corpus_matches_python_misaki() {
        let g2p = G2p::load().expect("load g2p");
        let mut misses = Vec::new();
        let mut total = 0;
        for line in SAMPLE.lines() {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            let text = row["text"].as_str().unwrap();
            total += 1;
            if g2p.phonemize(text, "❓") != row["phonemes"].as_str().unwrap() {
                misses.push(text.to_owned());
            }
        }
        let same = total - misses.len();
        assert!(
            same >= SAMPLE_FLOOR,
            "{same}/{total} identical; misses: {misses:#?}"
        );
    }

    /// Python misaki's output for both.
    #[test]
    #[ignore = "needs libespeak-ng; run with --ignored"]
    fn reads_arabic_indic_digits_like_ascii() {
        let g2p = G2p::load().expect("load g2p");
        assert_eq!(g2p.phonemize("٣ apples", "❓"), "θɹˈi ˈæpᵊlz");
        assert_eq!(g2p.phonemize("3 apples", "❓"), "θɹˈi ˈæpᵊlz");
    }
}
