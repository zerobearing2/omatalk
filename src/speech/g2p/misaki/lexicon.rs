//! en.py's `Lexicon`: gold and silver dictionary lookups, special cases,
//! and the -s, -ed and -ing stems. Numbers are in `number.rs`.
use super::number::currency_units;
use super::phonemes::{
    PRIMARY, SECONDARY, US_TAUS, apply_stress, ascii_digit, drop_end, is_alpha, len, lexicon_ords,
    lower, tail, upper,
};
use super::{Ctx, Tok};
use std::cmp::Ordering;
use unicode_normalization::UnicodeNormalization;

/// A lexicon compiled by `build.rs` into a sorted table and searched in
/// place: key -> phonemes, or tag -> phonemes for words that vary by tag.
#[derive(Clone, Copy)]
struct Dict(&'static [u8]);

#[derive(Clone, Copy)]
enum Entry {
    Plain(&'static str),
    Tagged(Tagged),
}

/// `tag 0x1f phonemes` pairs joined by 0x1e; 0x02 phonemes send the word to
/// espeak-ng. `build.rs` guarantees a DEFAULT.
#[derive(Clone, Copy)]
struct Tagged(&'static str);

impl Tagged {
    fn get(&self, tag: &str) -> Option<Option<&'static str>> {
        self.0.split('\x1e').find_map(|pair| {
            let (t, ps) = pair.split_once('\x1f')?;
            (t == tag).then_some((ps != "\x02").then_some(ps))
        })
    }

    fn default(&self) -> &'static str {
        self.get("DEFAULT").flatten().unwrap()
    }
}

impl Dict {
    fn u32_at(&self, i: usize) -> usize {
        u32::from_le_bytes(self.0[i * 4..i * 4 + 4].try_into().unwrap()) as usize
    }

    fn record(&self, i: usize) -> (&'static [u8], &'static [u8]) {
        let base = 4 * (self.u32_at(0) + 2);
        let record = &self.0[base + self.u32_at(i + 1)..base + self.u32_at(i + 2)];
        let nul = record.iter().position(|&b| b == 0).unwrap();
        (&record[..nul], &record[nul + 1..])
    }

    fn get(&self, word: &str) -> Option<Entry> {
        let (mut lo, mut hi) = (0, self.u32_at(0));
        let body = loop {
            if lo >= hi {
                return None;
            }
            let mid = (lo + hi) / 2;
            let (key, body) = self.record(mid);
            match key.cmp(word.as_bytes()) {
                Ordering::Less => lo = mid + 1,
                Ordering::Greater => hi = mid,
                Ordering::Equal => break std::str::from_utf8(body).unwrap(),
            }
        };
        Some(match body.strip_prefix('\x01') {
            Some(tags) => Entry::Tagged(Tagged(tags)),
            None => Entry::Plain(body),
        })
    }

    fn contains_key(&self, word: &str) -> bool {
        self.get(word).is_some()
    }
}

fn parent_tag(tag: &str) -> &str {
    if tag.starts_with("VB") {
        "VERB"
    } else if tag.starts_with("NN") {
        "NOUN"
    } else if tag.starts_with("ADV") || tag.starts_with("RB") {
        "ADV"
    } else if tag.starts_with("ADJ") || tag.starts_with("JJ") {
        "ADJ"
    } else {
        tag
    }
}

fn symbol_word(word: &str) -> Option<&'static str> {
    match word {
        "%" => Some("percent"),
        "&" => Some("and"),
        "+" => Some("plus"),
        "@" => Some("at"),
        _ => None,
    }
}

pub struct Lexicon {
    golds: Dict,
    silvers: Dict,
}

impl Lexicon {
    pub fn load() -> Lexicon {
        Lexicon {
            golds: Dict(include_bytes!(concat!(env!("OUT_DIR"), "/us_gold.lex"))),
            silvers: Dict(include_bytes!(concat!(env!("OUT_DIR"), "/us_silver.lex"))),
        }
    }

    /// `None` is the entry's DEFAULT.
    fn gold(&self, word: &str, tag: Option<&str>) -> Option<String> {
        match self.golds.get(word)? {
            Entry::Plain(s) => Some(s.to_owned()),
            Entry::Tagged(m) => match tag {
                None => Some(m.default().to_owned()),
                Some(tag) => m.get(tag).flatten().map(str::to_owned),
            },
        }
    }

    fn get_nnp(&self, word: &str) -> Option<String> {
        let mut ps = String::new();
        for c in word.chars().filter(|c| c.is_alphabetic()) {
            ps.push_str(&self.gold(&upper(&c.to_string()), None)?);
        }
        let ps = apply_stress(&ps, Some(0.0));
        Some(match ps.rfind(SECONDARY) {
            Some(i) => format!("{}{PRIMARY}{}", &ps[..i], &ps[i + SECONDARY.len_utf8()..]),
            None => ps,
        })
    }

    fn get_special_case(
        &self,
        word: &str,
        tag: &str,
        stress: Option<f64>,
        ctx: Ctx,
    ) -> Option<String> {
        let fv = ctx.future_vowel;
        if tag == "ADD" && (word == "." || word == "/") {
            let w = if word == "." { "dot" } else { "slash" };
            return self.lookup(w, None, Some(-0.5), Some(ctx));
        }
        if let Some(w) = symbol_word(word) {
            return self.lookup(w, None, None, Some(ctx));
        }
        let stripped = word.trim_matches('.');
        if stripped.contains('.')
            && is_alpha(&word.replace('.', ""))
            && word.split('.').map(len).max().unwrap_or(0) < 3
        {
            return self.get_nnp(word);
        }
        match word {
            "a" | "A" => {
                return Some(if tag == "DT" {
                    "ɐ".into()
                } else {
                    "ˈA".into()
                });
            }
            "am" | "Am" | "AM" => {
                if tag.starts_with("NN") {
                    return self.get_nnp(word);
                } else if fv.is_none() || word != "am" || stress.is_some_and(|s| s > 0.0) {
                    return self.gold("am", None);
                }
                return Some("ɐm".into());
            }
            "an" | "An" | "AN" => {
                if word == "AN" && tag.starts_with("NN") {
                    return self.get_nnp(word);
                }
                return Some("ɐn".into());
            }
            _ => {}
        }
        if word == "I" && tag == "PRP" {
            return Some(format!("{SECONDARY}I"));
        }
        if matches!(word, "by" | "By" | "BY") && parent_tag(tag) == "ADV" {
            return Some("bˈI".into());
        }
        if matches!(word, "to" | "To") || (word == "TO" && (tag == "TO" || tag == "IN")) {
            return match fv {
                None => self.gold("to", None),
                Some(false) => Some("tə".into()),
                Some(true) => Some("tʊ".into()),
            };
        }
        if matches!(word, "in" | "In") || (word == "IN" && tag != "NNP") {
            let s = if fv.is_none() || tag != "IN" {
                PRIMARY.to_string()
            } else {
                String::new()
            };
            return Some(format!("{s}ɪn"));
        }
        if matches!(word, "the" | "The") || (word == "THE" && tag == "DT") {
            return Some(if fv == Some(true) {
                "ði".into()
            } else {
                "ðə".into()
            });
        }
        if tag == "IN" && matches!(lower(word).as_str(), "vs" | "vs.") {
            return self.lookup("versus", None, None, Some(ctx));
        }
        if matches!(word, "used" | "Used" | "USED") {
            if (tag == "VBD" || tag == "JJ") && ctx.future_to {
                return self.gold("used", Some("VBD"));
            }
            return self.gold("used", None);
        }
        None
    }

    fn is_known(&self, word: &str) -> bool {
        if self.golds.contains_key(word)
            || symbol_word(word).is_some()
            || self.silvers.contains_key(word)
        {
            return true;
        }
        if !is_alpha(word) || !lexicon_ords(word) {
            return false;
        }
        if len(word) == 1 {
            return true;
        }
        if word == upper(word) && self.golds.contains_key(&lower(word)) {
            return true;
        }
        let rest = tail(word);
        rest == upper(&rest)
    }

    pub(super) fn lookup(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f64>,
        ctx: Option<Ctx>,
    ) -> Option<String> {
        let mut word = word.to_string();
        let mut is_nnp: Option<bool> = None;
        if word == upper(&word) && !self.golds.contains_key(&word) {
            word = lower(&word);
            is_nnp = Some(tag == Some("NNP"));
        }
        let mut entry = self.golds.get(&word);
        if entry.is_none() && is_nnp != Some(true) {
            entry = self.silvers.get(&word);
        }
        let ps: Option<String> = match entry {
            None => None,
            Some(Entry::Plain(s)) => Some(s.to_owned()),
            Some(Entry::Tagged(m)) => {
                let tag =
                    if ctx.is_some_and(|c| c.future_vowel.is_none()) && m.get("None").is_some() {
                        Some("None")
                    } else if tag.and_then(|t| m.get(t)).is_none() {
                        tag.map(parent_tag)
                    } else {
                        tag
                    };
                match tag.and_then(|t| m.get(t)) {
                    Some(ps) => ps.map(str::to_owned),
                    None => Some(m.default().to_owned()),
                }
            }
        };
        if (ps.is_none() || (is_nnp == Some(true) && !ps.as_ref().unwrap().contains(PRIMARY)))
            && let Some(nnp) = self.get_nnp(&word)
        {
            return Some(nnp);
        }
        ps.map(|ps| apply_stress(&ps, stress))
    }

    pub(super) fn suffix_s(stem: &str) -> String {
        match stem.chars().last() {
            Some(c) if "ptkfθ".contains(c) => format!("{stem}s"),
            Some(c) if "szʃʒʧʤ".contains(c) => format!("{stem}ᵻz"),
            _ => format!("{stem}z"),
        }
    }

    pub(super) fn stem_s(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f64>,
        ctx: Option<Ctx>,
    ) -> Option<String> {
        if len(word) < 3 || !word.ends_with('s') {
            return None;
        }
        let stem = if !word.ends_with("ss") && self.is_known(&drop_end(word, 1)) {
            drop_end(word, 1)
        } else if (word.ends_with("'s")
            || (len(word) > 4 && word.ends_with("es") && !word.ends_with("ies")))
            && self.is_known(&drop_end(word, 2))
        {
            drop_end(word, 2)
        } else if len(word) > 4
            && word.ends_with("ies")
            && self.is_known(&format!("{}y", drop_end(word, 3)))
        {
            format!("{}y", drop_end(word, 3))
        } else {
            return None;
        };
        let stem = self.lookup(&stem, tag, stress, ctx)?;
        if stem.is_empty() {
            None
        } else {
            Some(Self::suffix_s(&stem))
        }
    }

    pub(super) fn suffix_ed(stem: &str) -> Option<String> {
        let chars: Vec<char> = stem.chars().collect();
        let last = *chars.last()?;
        Some(if "pkfθʃsʧ".contains(last) {
            format!("{stem}t")
        } else if last == 'd' {
            format!("{stem}ᵻd")
        } else if last != 't' {
            format!("{stem}d")
        } else if chars.len() < 2 {
            format!("{stem}ɪd")
        } else if US_TAUS.contains(chars[chars.len() - 2]) {
            format!("{}ɾᵻd", drop_end(stem, 1))
        } else {
            format!("{stem}ᵻd")
        })
    }

    fn stem_ed(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f64>,
        ctx: Option<Ctx>,
    ) -> Option<String> {
        if len(word) < 4 || !word.ends_with('d') {
            return None;
        }
        let stem = if !word.ends_with("dd") && self.is_known(&drop_end(word, 1)) {
            drop_end(word, 1)
        } else if len(word) > 4
            && word.ends_with("ed")
            && !word.ends_with("eed")
            && self.is_known(&drop_end(word, 2))
        {
            drop_end(word, 2)
        } else {
            return None;
        };
        Self::suffix_ed(&self.lookup(&stem, tag, stress, ctx)?)
    }

    pub(super) fn suffix_ing(stem: &str) -> Option<String> {
        let chars: Vec<char> = stem.chars().collect();
        if chars.is_empty() {
            return None;
        }
        if chars.len() > 1
            && chars[chars.len() - 1] == 't'
            && US_TAUS.contains(chars[chars.len() - 2])
        {
            return Some(format!("{}ɾɪŋ", drop_end(stem, 1)));
        }
        Some(format!("{stem}ɪŋ"))
    }

    fn stem_ing(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f64>,
        ctx: Option<Ctx>,
    ) -> Option<String> {
        if len(word) < 5 || !word.ends_with("ing") {
            return None;
        }
        let base = drop_end(word, 3);
        let doubled = {
            let c: Vec<char> = word.chars().collect();
            let n = c.len();
            word.ends_with("cking")
                || (n >= 5 && c[n - 4] == c[n - 5] && "bcdgklmnprstvxz".contains(c[n - 4]))
        };
        let stem = if len(word) > 5 && self.is_known(&base) {
            base
        } else if self.is_known(&format!("{base}e")) {
            format!("{base}e")
        } else if len(word) > 5 && doubled && self.is_known(&drop_end(word, 4)) {
            drop_end(word, 4)
        } else {
            return None;
        };
        Self::suffix_ing(&self.lookup(&stem, tag, stress, ctx)?)
    }

    fn get_word(&self, word: &str, tag: &str, stress: Option<f64>, ctx: Ctx) -> Option<String> {
        if let Some(ps) = self.get_special_case(word, tag, stress, ctx) {
            return Some(ps);
        }
        let wl = lower(word);
        let mut word = word.to_string();
        let rest = tail(&word);
        if len(&word) > 1
            && is_alpha(&word.replace('\'', ""))
            && word != wl
            && (tag != "NNP" || len(&word) > 7)
            && !self.golds.contains_key(&word)
            && !self.silvers.contains_key(&word)
            && (word == upper(&word) || rest == lower(&rest))
            && (self.golds.contains_key(&wl)
                || self.silvers.contains_key(&wl)
                || self.stem_s(&wl, Some(tag), stress, Some(ctx)).is_some()
                || self.stem_ed(&wl, Some(tag), stress, Some(ctx)).is_some()
                || self.stem_ing(&wl, Some(tag), stress, Some(ctx)).is_some())
        {
            word = wl;
        }
        if self.is_known(&word) {
            return self.lookup(&word, Some(tag), stress, Some(ctx));
        }
        if word.ends_with("s'") {
            let w = format!("{}'s", drop_end(&word, 2));
            if self.is_known(&w) {
                return self.lookup(&w, Some(tag), stress, Some(ctx));
            }
        }
        if word.ends_with('\'') && self.is_known(&drop_end(&word, 1)) {
            return self.lookup(&drop_end(&word, 1), Some(tag), stress, Some(ctx));
        }
        if let Some(ps) = self.stem_s(&word, Some(tag), stress, Some(ctx)) {
            return Some(ps);
        }
        if let Some(ps) = self.stem_ed(&word, Some(tag), stress, Some(ctx)) {
            return Some(ps);
        }
        self.stem_ing(&word, Some(tag), Some(stress.unwrap_or(0.5)), Some(ctx))
    }

    fn append_currency(&self, ps: String, currency: Option<&str>) -> String {
        match currency.and_then(currency_units) {
            Some((unit, _)) => match self.stem_s(&format!("{unit}s"), None, None, None) {
                Some(c) => format!("{ps} {c}"),
                None => ps,
            },
            None => ps,
        }
    }

    pub(super) fn call(&self, tk: &Tok, ctx: Ctx) -> Option<String> {
        let word = tk
            .alias
            .as_deref()
            .unwrap_or(&tk.text)
            .replace(['‘', '’'], "'");
        let word: String = word.nfkc().map(ascii_digit).collect();
        let stress = if word == lower(&word) {
            None
        } else if word == upper(&word) {
            Some(2.0)
        } else {
            Some(0.5)
        };
        if let Some(ps) = self.get_word(&word, &tk.tag, stress, ctx) {
            return Some(self.append_currency(ps, tk.currency.as_deref()));
        }
        if Self::is_number(&word, tk.is_head) {
            return self.get_number(&word, tk.currency.as_deref(), tk.is_head);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, Lexicon};
    use serde_json::Value;
    use std::collections::BTreeMap;

    fn read(entry: Option<Entry>) -> Option<Value> {
        Some(match entry? {
            Entry::Plain(ps) => Value::from(ps),
            Entry::Tagged(m) => {
                let tags =
                    m.0.split('\x1e')
                        .map(|pair| pair.split_once('\x1f').unwrap().0);
                Value::Object(
                    tags.map(|t| {
                        (
                            t.to_owned(),
                            m.get(t).unwrap().map_or(Value::Null, Value::from),
                        )
                    })
                    .collect(),
                )
            }
        })
    }

    #[test]
    fn every_json_entry_reads_back_from_its_table() {
        let lexicon = Lexicon::load();
        for (dict, json) in [
            (lexicon.golds, include_str!("../../../../data/us_gold.json")),
            (
                lexicon.silvers,
                include_str!("../../../../data/us_silver.json"),
            ),
        ] {
            let raw: BTreeMap<String, Value> = serde_json::from_str(json).unwrap();
            for (word, value) in raw {
                assert_eq!(read(dict.get(&word)).as_ref(), Some(&value), "{word}");
            }
        }
    }

    #[test]
    fn tables_grow_case_variants_and_keep_json_keys_first() {
        let golds = Lexicon::load().golds;
        let plain = |w| match golds.get(w) {
            Some(Entry::Plain(ps)) => Some(ps),
            _ => None,
        };
        assert_eq!(plain("Apple"), Some("ˈæpᵊl"), "grown from apple");
        assert_eq!(
            plain("August"),
            Some("ˈɔɡəst"),
            "JSON key beats growth from august"
        );
        assert_eq!(plain("august"), Some("ɔɡˈʌst"));
        assert!(golds.get("zzqxv").is_none());
        let Some(Entry::Tagged(aa)) = golds.get("AA") else {
            panic!("AA is tagged")
        };
        assert_eq!(aa.default(), "ˈɑˌɑ");
        assert_eq!(
            aa.get("NOUN"),
            Some(None),
            "null sends the word to espeak-ng"
        );
        assert_eq!(aa.get("VERB"), None);
    }
}
