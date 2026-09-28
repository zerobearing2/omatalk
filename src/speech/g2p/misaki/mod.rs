//! Port of misaki 0.9.4 English G2P (misaki/en.py), the phonemizer Kokoro was
//! trained with. Tokens and Penn Treebank tags come from the caller.
mod lexicon;
mod number;
mod phonemes;

pub use lexicon::Lexicon;
use number::currency_units;
use phonemes::{
    CONSONANTS, NON_QUOTE_PUNCTS, PRIMARY, PUNCTS, SUBTOKEN_JUNKS, VOWELS, apply_stress, len,
    lower, stress_weight,
};

use fancy_regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

const PUNCT_TAGS: [&str; 11] = [
    ".", ",", "-LRB-", "-RRB-", "``", "\"\"", "''", ":", "$", "#", "NFP",
];
static SUBTOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^['‘’]+|\p{Lu}(?=\p{Lu}\p{Ll})|(?:^-)?(?:\d?[,.]?\d)+|[-_]+|['‘’]{2,}|\p{L}*?(?:['‘’]\p{L})*?\p{Ll}(?=\p{Lu})|\p{L}+(?:['‘’]\p{L})*|[^-_\p{L}'‘’\d]|['‘’]+$").unwrap()
});

#[derive(Clone, Copy, Default)]
pub struct Ctx {
    future_vowel: Option<bool>,
    future_to: bool,
}

#[derive(Clone, Default)]
pub struct Tok {
    pub text: String,
    pub tag: String,
    pub ws: String,
    phonemes: Option<String>,
    currency: Option<String>,
    prespace: bool,
    is_head: bool,
    alias: Option<String>,
}

impl Tok {
    pub fn new(text: &str, tag: &str, ws: &str) -> Tok {
        Tok {
            text: text.into(),
            tag: tag.into(),
            ws: ws.into(),
            is_head: true,
            ..Default::default()
        }
    }
    fn resolved(&self) -> bool {
        self.alias.is_some() || self.phonemes.is_some()
    }
}

fn merge_tokens(tokens: &[Tok], unk: Option<&str>) -> Tok {
    let phonemes = unk.map(|unk| {
        let mut out = String::new();
        for tk in tokens {
            let ps = tk.phonemes.as_deref();
            if tk.prespace
                && !out.is_empty()
                && !out.ends_with(char::is_whitespace)
                && ps.is_some_and(|p| !p.is_empty())
            {
                out.push(' ');
            }
            out.push_str(ps.unwrap_or(unk));
        }
        out
    });
    let weight = |t: &Tok| -> usize {
        t.text
            .chars()
            .map(|c| {
                if lower(&c.to_string()) == c.to_string() {
                    1
                } else {
                    2
                }
            })
            .sum()
    };
    let mut best = &tokens[0];
    for t in &tokens[1..] {
        if weight(t) > weight(best) {
            best = t;
        }
    }
    let last = tokens.last().unwrap();
    let mut text: String = tokens[..tokens.len() - 1]
        .iter()
        .map(|t| format!("{}{}", t.text, t.ws))
        .collect();
    text.push_str(&last.text);
    Tok {
        text,
        tag: best.tag.clone(),
        ws: last.ws.clone(),
        phonemes,
        currency: tokens.iter().filter_map(|t| t.currency.clone()).max(),
        prespace: tokens[0].prespace,
        is_head: tokens[0].is_head,
        alias: None,
    }
}

const E2M: &[(&str, &str)] = &[
    ("ʔˌn\u{0329}", "ʔn"),
    ("ʔn\u{0329}", "ʔn"),
    ("a^ɪ", "I"),
    ("a^ʊ", "W"),
    ("d^ʒ", "ʤ"),
    ("e^ɪ", "A"),
    ("t^ʃ", "ʧ"),
    ("ɔ^ɪ", "Y"),
    ("ə^l", "ᵊl"),
    ("ʲo", "jo"),
    ("ʲə", "jə"),
    ("ʲ", ""),
    ("e", "A"),
    ("ɚ", "əɹ"),
    ("r", "ɹ"),
    ("x", "k"),
    ("ç", "k"),
    ("ɐ", "ə"),
    ("ɬ", "l"),
    ("\u{0303}", ""),
];

static SYLLABIC: LazyLock<Regex> = LazyLock::new(|| Regex::new("(\\S)\u{0329}").unwrap());

fn fallback(espeak: &mut impl FnMut(&str) -> String, tk: &Tok) -> Option<String> {
    let ps = espeak(&tk.text);
    let mut ps = ps.split('\n').next().unwrap_or("").trim().to_string();
    for (old, new) in E2M {
        ps = ps.replace(old, new);
    }
    ps = SYLLABIC.replace_all(&ps, "ᵊ$1").replace('\u{0329}', "");
    ps = ps
        .replace("o^ʊ", "O")
        .replace("ɜːɹ", "ɜɹ")
        .replace("ɜː", "ɜɹ")
        .replace("ɪə", "iə")
        .replace('ː', "");
    ps = ps.replace('o', "ɔ").replace('ɾ', "T").replace('ʔ', "t");
    Some(ps.replace('^', ""))
}

enum Word {
    One(Tok),
    Many(Vec<Tok>),
}

fn retokenize(tokens: Vec<Tok>) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut currency: Option<String> = None;
    let n = tokens.len();
    for i in 0..n {
        let token = &tokens[i];
        let mut tks: Vec<Tok> = if !token.resolved() {
            SUBTOKEN
                .find_iter(&token.text)
                .map(|m| Tok::new(m.unwrap().as_str(), &token.tag, ""))
                .collect()
        } else {
            vec![token.clone()]
        };
        if tks.is_empty() {
            continue;
        }
        tks.last_mut().unwrap().ws = token.ws.clone();
        let count = tks.len();
        for j in 0..count {
            let (prev_last, next_first) = (
                if j > 0 {
                    tks[j - 1].text.chars().last()
                } else {
                    None
                },
                tks.get(j + 1).and_then(|t| t.text.chars().next()),
            );
            let tk = &mut tks[j];
            if tk.resolved() {
            } else if tk.tag == "$" && currency_units(&tk.text).is_some() {
                currency = Some(tk.text.clone());
                tk.phonemes = Some(String::new());
            } else if tk.tag == ":" && (tk.text == "-" || tk.text == "–") {
                tk.phonemes = Some("—".into());
            } else if PUNCT_TAGS.contains(&tk.tag.as_str())
                && !tk.text.chars().all(|c| {
                    lower(&c.to_string())
                        .chars()
                        .all(|l| l.is_ascii_lowercase())
                })
            {
                tk.phonemes = Some(match tk.tag.as_str() {
                    "-LRB-" => "(".into(),
                    "-RRB-" => ")".into(),
                    "``" => "“".into(),
                    "\"\"" | "''" => "”".into(),
                    _ => tk.text.chars().filter(|c| PUNCTS.contains(*c)).collect(),
                });
            } else if currency.is_some() {
                if tk.tag != "CD" {
                    currency = None;
                } else if j + 1 == count && (i + 1 == n || tokens[i + 1].tag != "CD") {
                    tk.currency = currency.clone();
                }
            } else if 0 < j
                && j + 1 < count
                && tk.text == "2"
                && prev_last.is_some_and(char::is_alphabetic)
                && next_first.is_some_and(char::is_alphabetic)
            {
                tk.alias = Some("to".into());
            }
        }
        for mut tk in tks {
            if tk.resolved() {
                words.push(Word::One(tk));
            } else if let Some(Word::Many(group)) = words
                .last_mut()
                .filter(|w| matches!(w, Word::Many(g) if g.last().unwrap().ws.is_empty()))
            {
                tk.is_head = false;
                group.push(tk);
            } else if !tk.ws.is_empty() {
                words.push(Word::One(tk));
            } else {
                words.push(Word::Many(vec![tk]));
            }
        }
    }
    words
        .into_iter()
        .map(|w| match w {
            Word::Many(mut g) if g.len() == 1 => Word::One(g.pop().unwrap()),
            w => w,
        })
        .collect()
}

fn token_context(ctx: Ctx, ps: Option<&str>, token: &Tok) -> Ctx {
    let mut vowel = ctx.future_vowel;
    if let Some(ps) = ps.filter(|p| !p.is_empty())
        && let Some(c) = ps.chars().find(|c| {
            VOWELS.contains(*c) || CONSONANTS.contains(*c) || NON_QUOTE_PUNCTS.contains(*c)
        })
    {
        vowel = if NON_QUOTE_PUNCTS.contains(c) {
            None
        } else {
            Some(VOWELS.contains(c))
        };
    }
    let t = token.text.as_str();
    Ctx {
        future_vowel: vowel,
        future_to: t == "to"
            || t == "To"
            || (t == "TO" && (token.tag == "TO" || token.tag == "IN")),
    }
}

fn resolve_tokens(tokens: &mut [Tok]) {
    let mut text: String = tokens[..tokens.len() - 1]
        .iter()
        .map(|t| format!("{}{}", t.text, t.ws))
        .collect();
    text.push_str(&tokens.last().unwrap().text);
    let classes: HashSet<u8> = text
        .chars()
        .filter(|c| !SUBTOKEN_JUNKS.contains(*c))
        .map(|c| {
            if c.is_alphabetic() {
                0
            } else if c.is_ascii_digit() {
                1
            } else {
                2
            }
        })
        .collect();
    let prespace = text.contains(' ') || text.contains('/') || classes.len() > 1;
    let last = tokens.len() - 1;
    for (i, tk) in tokens.iter_mut().enumerate() {
        if tk.phonemes.is_none() {
            if i == last && len(&tk.text) == 1 && NON_QUOTE_PUNCTS.contains(tk.text.as_str()) {
                tk.phonemes = Some(tk.text.clone());
            } else if tk.text.chars().all(|c| SUBTOKEN_JUNKS.contains(c)) {
                tk.phonemes = Some(String::new());
            }
        } else if i > 0 {
            tk.prespace = prespace;
        }
    }
    if prespace {
        return;
    }
    let mut indices: Vec<(bool, usize, usize)> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            t.phonemes
                .as_deref()
                .filter(|p| !p.is_empty())
                .map(|p| (p.contains(PRIMARY), stress_weight(p), i))
        })
        .collect();
    if indices.len() == 2 && len(&tokens[indices[0].2].text) == 1 {
        let i = indices[1].2;
        tokens[i].phonemes = tokens[i]
            .phonemes
            .as_deref()
            .map(|p| apply_stress(p, Some(-0.5)));
        return;
    }
    let primaries = indices.iter().filter(|x| x.0).count();
    if indices.len() < 2 || primaries <= indices.len().div_ceil(2) {
        return;
    }
    indices.sort();
    let half = indices.len() / 2;
    for &(_, _, i) in &indices[..half] {
        tokens[i].phonemes = tokens[i]
            .phonemes
            .as_deref()
            .map(|p| apply_stress(p, Some(-0.5)));
    }
}

/// Subtokens past which `read_group` skips its cubic search (64 took ~1.5 ms,
/// 1000 took 29 s, release). misaki has no cap; the largest group in the
/// parity samples is 17.
const MAX_GROUP: usize = 64;

/// Reads a whitespace-free group from the right, taking the longest run of
/// subtokens the lexicon knows each time. False when a subtoken is neither
/// known nor junk, so espeak must read the whole group.
fn read_group(lexicon: &Lexicon, group: &mut [Tok], ctx: &mut Ctx) -> bool {
    if group.len() > MAX_GROUP {
        return false;
    }
    let (mut left, mut right) = (0, group.len());
    while left < right {
        let tk = if group[left..right].iter().any(Tok::resolved) {
            None
        } else {
            Some(merge_tokens(&group[left..right], None))
        };
        let ps = tk.as_ref().and_then(|t| lexicon.call(t, *ctx));
        if let Some(ps) = ps {
            for x in &mut group[left + 1..right] {
                x.phonemes = Some(String::new());
            }
            *ctx = token_context(*ctx, Some(&ps), tk.as_ref().unwrap());
            group[left].phonemes = Some(ps);
            right = left;
            left = 0;
        } else if left + 1 < right {
            left += 1;
        } else {
            right -= 1;
            let tk = &mut group[right];
            if tk.phonemes.is_none() {
                if tk.text.chars().all(|c| SUBTOKEN_JUNKS.contains(c)) {
                    tk.phonemes = Some(String::new());
                } else {
                    return false;
                }
            }
            left = 0;
        }
    }
    true
}

/// misaki's `[text](feature)` markdown (per-token stress and number flags) is
/// not supported. `espeak` reads what the lexicon cannot, returning
/// espeak-ng's tie-marked IPA as `Espeak::phonemize_tied` does.
pub fn g2p(
    lexicon: &Lexicon,
    tokens: Vec<Tok>,
    unk: &str,
    mut espeak: impl FnMut(&str) -> String,
) -> String {
    let mut words = retokenize(tokens);
    let mut ctx = Ctx::default();
    for w in words.iter_mut().rev() {
        match w {
            Word::One(tk) => {
                if tk.phonemes.is_none() {
                    tk.phonemes = lexicon.call(tk, ctx);
                }
                if tk.phonemes.is_none() {
                    tk.phonemes = fallback(&mut espeak, tk);
                }
                ctx = token_context(ctx, tk.phonemes.as_deref(), tk);
            }
            Word::Many(group) => {
                if read_group(lexicon, group, &mut ctx) {
                    resolve_tokens(group);
                } else {
                    let merged = merge_tokens(group, None);
                    group[0].phonemes = fallback(&mut espeak, &merged);
                    for x in &mut group[1..] {
                        x.phonemes = Some(String::new());
                    }
                }
            }
        }
    }
    let mut out = String::new();
    for w in words {
        let tk = match w {
            Word::One(tk) => tk,
            Word::Many(group) => merge_tokens(&group, Some(unk)),
        };
        let ps = tk.phonemes.unwrap_or_else(|| unk.to_string());
        out.push_str(&ps.replace('ɾ', "T").replace('ʔ', "t"));
        out.push_str(&tk.ws);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Ctx, Lexicon, Tok, Word, read_group, retokenize};

    fn read(lexicon: &Lexicon, text: &str) -> bool {
        let Some(Word::Many(mut group)) = retokenize(vec![Tok::new(text, "NN", "")]).pop() else {
            panic!("{text} is not a group");
        };
        read_group(lexicon, &mut group, &mut Ctx::default())
    }

    /// The sub-range search is cubic in the group's length: 1000 subtokens
    /// took minutes.
    #[test]
    fn long_groups_go_to_espeak_without_the_search() {
        let lexicon = Lexicon::load().unwrap();
        assert!(read(&lexicon, &"aB".repeat(8)));
        assert!(!read(&lexicon, &"aB".repeat(1000)));
    }
}
