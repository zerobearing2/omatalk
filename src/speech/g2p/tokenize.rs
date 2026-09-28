//! Port of the spaCy 3.8 Tokenizer (spacy/tokenizer.pyx) with en_core_web_sm's
//! affix patterns and special cases, from the embedded `data/tokenizer.json`
//! (tools/parity/export_tokenizer.py).
use fancy_regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

use crate::speech::LoadError;

pub struct Tokenizer {
    prefix: Vec<Regex>,
    suffix: Vec<Regex>,
    infix: Regex,
    url: Regex,
    specials: HashMap<String, Vec<String>>,
    phrases: HashMap<String, Vec<Vec<String>>>,
}

#[derive(Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
    space: bool,
}

/// Python's `str.isspace()`, which also counts the ASCII information
/// separators. spaCy strips these from the start of the text.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

fn bad(why: impl std::fmt::Display) -> LoadError {
    LoadError(format!("tokenizer.json: {why}"))
}

fn regex(v: &Value) -> Result<Regex, LoadError> {
    Regex::new(v.as_str().ok_or_else(|| bad("pattern is not a string"))?).map_err(bad)
}

// An alternation split into runs of consecutive alternatives that do or do not
// need lookaround. Lookaround-free runs compile to the regex crate's automata
// instead of fancy-regex's backtracker, which dominated the runtime.
fn runs(v: &Value) -> Result<Vec<Regex>, LoadError> {
    let fancy = |a: &str| ["(?=", "(?!", "(?<=", "(?<!"].iter().any(|l| a.contains(l));
    let alts: Vec<&str> = v
        .as_array()
        .ok_or_else(|| bad("affixes are not a list"))?
        .iter()
        .map(|a| a.as_str().ok_or_else(|| bad("affix is not a string")))
        .collect::<Result<_, _>>()?;
    alts.chunk_by(|a, b| fancy(a) == fancy(b))
        .map(|run| Regex::new(&run.join("|")).map_err(bad))
        .collect()
}

impl Tokenizer {
    pub fn load() -> Result<Tokenizer, LoadError> {
        let json = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/tokenizer.json"));
        let data: Value = serde_json::from_str(json).map_err(bad)?;
        let orths = |v: &Value| -> Option<Vec<String>> {
            v.as_array()?
                .iter()
                .map(|o| o.as_str().map(str::to_string))
                .collect()
        };
        let specials: HashMap<String, Vec<String>> = data["rules"]
            .as_object()
            .ok_or_else(|| bad("rules are not an object"))?
            .iter()
            .map(|(k, v)| Ok((k.clone(), orths(v).ok_or_else(|| bad(format!("rule {k}")))?)))
            .collect::<Result<_, LoadError>>()?;
        let mut t = Tokenizer {
            prefix: runs(&data["prefix"])?,
            suffix: runs(&data["suffix"])?,
            infix: regex(&data["infix"])?,
            url: regex(&data["url_match"])?,
            specials,
            phrases: HashMap::new(),
        };
        let mut phrases: HashMap<String, Vec<Vec<String>>> = HashMap::new();
        for key in t.specials.keys() {
            if t.find_prefix(key) == 0
                && t.find_suffix(key) == 0
                && t.infix.find_iter(key).next().is_none()
                && !key.contains(' ')
            {
                continue;
            }
            let pattern: Vec<String> = t
                .tokenize_affixes(key, false)
                .iter()
                .map(|s| key[s.start..s.end].to_string())
                .collect();
            phrases.entry(pattern[0].clone()).or_default().push(pattern);
        }
        t.phrases = phrases;
        Ok(t)
    }

    /// (token text, trailing whitespace) pairs, as spaCy's `Doc` has them.
    pub fn tokenize(&self, text: &str) -> Vec<(String, String)> {
        let spans = self.apply_special_cases(text, self.tokenize_affixes(text, true));
        spans
            .into_iter()
            .map(|s| {
                (
                    text[s.start..s.end].to_string(),
                    if s.space { " " } else { "" }.to_string(),
                )
            })
            .collect()
    }

    // Every alternative is `^`-anchored, so the first run to match picks the
    // same alternative the joined pattern would.
    fn find_prefix(&self, s: &str) -> usize {
        self.prefix
            .iter()
            .find_map(|r| r.find(s).unwrap())
            .map_or(0, |m| m.end() - m.start())
    }

    // Every alternative is `$`-anchored, so the joined pattern's leftmost match
    // is the leftmost start over all runs.
    fn find_suffix(&self, s: &str) -> usize {
        self.suffix
            .iter()
            .filter_map(|r| r.find(s).unwrap())
            .map(|m| s.len() - m.start())
            .max()
            .unwrap_or(0)
    }

    fn tokenize_affixes(&self, text: &str, specials: bool) -> Vec<Span> {
        let mut doc = Vec::new();
        let Some(first) = text.chars().next() else {
            return doc;
        };
        let mut in_ws = is_space(first);
        let mut start = 0;
        for (i, c) in text.char_indices() {
            if is_space(c) != in_ws {
                if start < i {
                    self.tokenize_span(&mut doc, text, start, i, specials);
                }
                if c == ' ' {
                    doc.last_mut().unwrap().space = true;
                    start = i + 1;
                } else {
                    start = i;
                }
                in_ws = !in_ws;
            }
        }
        if start < text.len() {
            self.tokenize_span(&mut doc, text, start, text.len(), specials);
        }
        doc
    }

    fn push_special(&self, doc: &mut Vec<Span>, orths: &[String], mut at: usize) {
        for o in orths {
            doc.push(Span {
                start: at,
                end: at + o.len(),
                space: false,
            });
            at += o.len();
        }
    }

    fn tokenize_span(
        &self,
        doc: &mut Vec<Span>,
        text: &str,
        start: usize,
        end: usize,
        specials: bool,
    ) {
        if specials && let Some(orths) = self.specials.get(&text[start..end]) {
            self.push_special(doc, orths, start);
            return;
        }
        let (mut lo, mut hi) = (start, end);
        let mut prefixes = Vec::new();
        let mut suffixes = Vec::new();
        let is_special = |a: usize, b: usize| specials && self.specials.contains_key(&text[a..b]);
        let mut last_size = 0;
        while lo < hi && hi - lo != last_size {
            if is_special(lo, hi) {
                break;
            }
            last_size = hi - lo;
            let pre = self.find_prefix(&text[lo..hi]);
            if pre != 0 && lo + pre < hi && is_special(lo + pre, hi) {
                prefixes.push((lo, lo + pre));
                lo += pre;
                break;
            }
            let suf = self.find_suffix(&text[lo + pre..hi]);
            if suf != 0 && lo < hi - suf && is_special(lo, hi - suf) {
                suffixes.push((hi - suf, hi));
                hi -= suf;
                break;
            }
            if pre != 0 && suf != 0 && pre + suf <= hi - lo {
                prefixes.push((lo, lo + pre));
                suffixes.push((hi - suf, hi));
                lo += pre;
                hi -= suf;
            } else if pre != 0 {
                prefixes.push((lo, lo + pre));
                lo += pre;
            } else if suf != 0 {
                suffixes.push((hi - suf, hi));
                hi -= suf;
            }
        }
        let token = |a, b| Span {
            start: a,
            end: b,
            space: false,
        };
        doc.extend(prefixes.iter().map(|&(a, b)| token(a, b)));
        if lo < hi {
            let s = &text[lo..hi];
            if let Some(orths) = self.specials.get(s).filter(|_| specials) {
                self.push_special(doc, orths, lo);
            } else if self.url.is_match(s).unwrap() {
                doc.push(token(lo, hi));
            } else {
                let mut at = 0;
                for m in self.infix.find_iter(s) {
                    let m = m.unwrap();
                    if m.start() == 0 {
                        continue;
                    }
                    if m.start() != at {
                        doc.push(token(lo + at, lo + m.start()));
                    }
                    if m.start() != m.end() {
                        doc.push(token(lo + m.start(), lo + m.end()));
                    }
                    at = m.end();
                }
                if at < s.len() {
                    doc.push(token(lo + at, hi));
                }
            }
        }
        doc.extend(suffixes.iter().rev().map(|&(a, b)| token(a, b)));
    }

    fn apply_special_cases(&self, text: &str, doc: Vec<Span>) -> Vec<Span> {
        let orth = |s: &Span| &text[s.start..s.end];
        let mut matches = Vec::new();
        for i in 0..doc.len() {
            for pattern in self.phrases.get(orth(&doc[i])).into_iter().flatten() {
                let end = i + pattern.len();
                if end <= doc.len() && doc[i..end].iter().zip(pattern).all(|(s, p)| orth(s) == p) {
                    matches.push((i, end));
                }
            }
        }
        if matches.is_empty() {
            return doc;
        }
        // Longest first, then leftmost; a span survives when neither end token is taken.
        matches.sort_by_key(|&(s, e)| (std::cmp::Reverse(e - s), s));
        let mut seen = vec![false; doc.len()];
        let mut starts: HashMap<usize, usize> = HashMap::new();
        for (s, e) in matches {
            if !seen[s] && !seen[e - 1] {
                starts.insert(s, e);
            }
            seen[s..e].iter_mut().for_each(|x| *x = true);
        }
        let mut out = Vec::with_capacity(doc.len());
        let mut i = 0;
        while i < doc.len() {
            let Some(&e) = starts.get(&i) else {
                out.push(doc[i]);
                i += 1;
                continue;
            };
            match self.specials.get(&text[doc[i].start..doc[e - 1].end]) {
                Some(orths) => {
                    self.push_special(&mut out, orths, doc[i].start);
                    out.last_mut().unwrap().space = doc[e - 1].space;
                }
                None => out.extend_from_slice(&doc[i..e]),
            }
            i = e;
        }
        out
    }
}
