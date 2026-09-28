//! en.py's module-level helpers: phoneme and punctuation classes, stress
//! rewriting, and the Python `str` semantics the port leans on.
use fancy_regex::Regex;
use std::sync::LazyLock;

pub(super) const PRIMARY: char = 'ˈ';
pub(super) const SECONDARY: char = 'ˌ';
pub(super) const VOWELS: &str = "AIOQWYaiuæɑɒɔəɛɜɪʊʌᵻ";
pub(super) const CONSONANTS: &str = "bdfhjklmnpstvwzðŋɡɹɾʃʒʤʧθ";
const DIPHTHONGS: &str = "AIOQWYʤʧ";
pub(super) const US_TAUS: &str = "AIOWYiuæɑəɛɪɹʊʌ";
pub(super) const PUNCTS: &str = ";:,.!?—…\"“”";
pub(super) const NON_QUOTE_PUNCTS: &str = ";:,.!?—…";
pub(super) const SUBTOKEN_JUNKS: &str = "',-._‘’/";

// Python str semantics the port leans on.
pub(super) fn lower(s: &str) -> String {
    s.to_lowercase()
}
pub(super) fn upper(s: &str) -> String {
    s.to_uppercase()
}
pub(super) fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(lower(chars.as_str()).chars())
            .collect(),
        None => String::new(),
    }
}
pub(super) fn is_alpha(s: &str) -> bool {
    !s.is_empty() && s.chars().all(char::is_alphabetic)
}
/// Python `str.isdigit` characters that NFKC keeps and that are not Nd, with
/// the first one's value (unicodedata 16.0).
const OTHER_DIGITS: [(char, char, u32); 10] = [
    ('\u{1369}', '\u{1371}', 1),
    ('\u{19da}', '\u{19da}', 1),
    ('\u{24f5}', '\u{24fd}', 1),
    ('\u{24ff}', '\u{24ff}', 0),
    ('\u{2776}', '\u{277e}', 1),
    ('\u{2780}', '\u{2788}', 1),
    ('\u{278a}', '\u{2792}', 1),
    ('\u{10a40}', '\u{10a43}', 1),
    ('\u{10e60}', '\u{10e68}', 1),
    ('\u{11052}', '\u{1105a}', 1),
];

/// misaki's `numeric_if_needed`: a digit NFKC left non-ASCII (Arabic-Indic,
/// Devanagari, ❶) becomes its ASCII digit.
pub(super) fn ascii_digit(c: char) -> char {
    static ND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{Nd}$").unwrap());
    let is_nd = |c: char| ND.is_match(c.encode_utf8(&mut [0; 4])).unwrap();
    if c.is_ascii() || !c.is_numeric() {
        return c;
    }
    let value = match OTHER_DIGITS
        .iter()
        .find(|(lo, hi, _)| (*lo..=*hi).contains(&c))
    {
        Some((lo, _, first)) => first + (c as u32 - *lo as u32),
        // Nd digits come in runs of ten from zero, some runs adjacent.
        None if is_nd(c) => {
            let before = (1..)
                .take_while(|k| char::from_u32(c as u32 - k).is_some_and(is_nd))
                .count();
            before as u32 % 10
        }
        None => return c,
    };
    char::from_digit(value, 10).unwrap()
}

pub(super) fn is_digit(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}
pub(super) fn len(s: &str) -> usize {
    s.chars().count()
}
pub(super) fn drop_end(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    chars[..chars.len().saturating_sub(n)].iter().collect()
}
pub(super) fn tail(s: &str) -> String {
    s.chars().skip(1).collect()
}
pub(super) fn lexicon_ords(s: &str) -> bool {
    s.chars()
        .all(|c| c == '\'' || c == '-' || c.is_ascii_alphabetic())
}

fn restress(ps: &str) -> String {
    let chars: Vec<char> = ps.chars().collect();
    let mut items: Vec<(f64, char)> = chars
        .iter()
        .enumerate()
        .map(|(i, c)| (i as f64, *c))
        .collect();
    for (i, c) in chars.iter().enumerate() {
        if (*c == PRIMARY || *c == SECONDARY)
            && let Some(j) = (i..chars.len()).find(|&j| VOWELS.contains(chars[j]))
        {
            items[i].0 = j as f64 - 0.5;
        }
    }
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
    items.into_iter().map(|(_, c)| c).collect()
}

fn has_stress(ps: &str) -> bool {
    ps.contains(PRIMARY) || ps.contains(SECONDARY)
}

fn has_vowel(ps: &str) -> bool {
    ps.chars().any(|c| VOWELS.contains(c))
}

pub(super) fn apply_stress(ps: &str, stress: Option<f64>) -> String {
    let Some(s) = stress else {
        return ps.to_string();
    };
    if s < -1.0 {
        ps.replace([PRIMARY, SECONDARY], "")
    } else if s == -1.0 || ((s == 0.0 || s == -0.5) && ps.contains(PRIMARY)) {
        ps.replace(SECONDARY, "")
            .replace(PRIMARY, &SECONDARY.to_string())
    } else if (s == 0.0 || s == 0.5 || s == 1.0) && !has_stress(ps) {
        if !has_vowel(ps) {
            ps.to_string()
        } else {
            restress(&format!("{SECONDARY}{ps}"))
        }
    } else if s >= 1.0 && !ps.contains(PRIMARY) && ps.contains(SECONDARY) {
        ps.replace(SECONDARY, &PRIMARY.to_string())
    } else if s > 1.0 && !has_stress(ps) {
        if !has_vowel(ps) {
            ps.to_string()
        } else {
            restress(&format!("{PRIMARY}{ps}"))
        }
    } else {
        ps.to_string()
    }
}

pub(super) fn stress_weight(ps: &str) -> usize {
    ps.chars()
        .map(|c| if DIPHTHONGS.contains(c) { 2 } else { 1 })
        .sum()
}
