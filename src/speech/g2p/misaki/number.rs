//! en.py's `Lexicon.get_number` and friends: digits, ordinals, years,
//! decimals and money read through the lexicon.
use super::super::numwords::{number, ordinalize, year};
use super::Lexicon;
use super::phonemes::{drop_end, is_digit, len};

const ORDINALS: [&str; 4] = ["st", "nd", "rd", "th"];

pub(super) fn currency_units(symbol: &str) -> Option<(&'static str, &'static str)> {
    match symbol {
        "$" => Some(("dollar", "cent")),
        "£" => Some(("pound", "pence")),
        "€" => Some(("euro", "cent")),
        _ => None,
    }
}

impl Lexicon {
    pub(super) fn is_number(word: &str, is_head: bool) -> bool {
        if !word.chars().any(|c| c.is_ascii_digit()) {
            return false;
        }
        let mut w = word;
        for s in ["ing", "'d", "ed", "'s", "st", "nd", "rd", "th", "s"] {
            if let Some(stripped) = w.strip_suffix(s) {
                w = stripped;
                break;
            }
        }
        w.chars().enumerate().all(|(i, c)| {
            c.is_ascii_digit() || c == ',' || c == '.' || (is_head && i == 0 && c == '-')
        })
    }

    pub(super) fn get_number(
        &self,
        word: &str,
        currency: Option<&str>,
        is_head: bool,
    ) -> Option<String> {
        let suffix_len = word
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_lowercase() || *c == '\'')
            .count();
        let suffix: String = word.chars().skip(len(word) - suffix_len).collect();
        let mut word = drop_end(word, suffix_len);
        let mut result: Vec<String> = Vec::new();
        if let Some(rest) = word.strip_prefix('-') {
            result.push(self.lookup("minus", None, None, None)?);
            word = rest.to_string();
        }
        let extend = |result: &mut Vec<String>, words: &str| {
            for w in words.split(|c: char| !c.is_ascii_lowercase()) {
                if w != "and" {
                    let stress = if w == "point" { Some(-2.0) } else { None };
                    result.push(self.lookup(w, None, stress, None).unwrap_or_default());
                }
            }
        };
        let num = |s: &str| number(s).1;
        let has_unit = currency.and_then(currency_units).is_some();
        if is_digit(&word) && ORDINALS.contains(&suffix.as_str()) {
            extend(&mut result, &ordinalize(&num(&word)));
        } else if result.is_empty() && len(&word) == 4 && !has_unit && is_digit(&word) {
            extend(&mut result, &year(word.parse().ok()?));
        } else if !is_head && !word.contains('.') {
            let n = word.replace(',', "");
            let d: Vec<char> = n.chars().collect();
            if d.first() == Some(&'0') || d.len() > 3 {
                for c in &d {
                    extend(&mut result, &num(&c.to_string()));
                }
            } else if d.len() == 3 && !n.ends_with("00") {
                extend(&mut result, &num(&d[0].to_string()));
                if d[1] == '0' {
                    result.push(self.lookup("O", None, Some(-2.0), None)?);
                    extend(&mut result, &num(&d[2].to_string()));
                } else {
                    extend(&mut result, &num(&d[1..].iter().collect::<String>()));
                }
            } else {
                extend(&mut result, &num(&n));
            }
        } else if word.matches('.').count() > 1 || !is_head {
            for part in word.replace(',', "").split('.') {
                let d: Vec<char> = part.chars().collect();
                if d.is_empty() {
                } else if d[0] == '0' || (d.len() != 2 && d[1..].iter().any(|c| *c != '0')) {
                    for c in &d {
                        extend(&mut result, &num(&c.to_string()));
                    }
                } else {
                    extend(&mut result, &num(part));
                }
            }
        } else if has_unit
            && (!word.contains('.') || word.split('.').nth(1).map(len).unwrap_or(0) < 3)
        {
            let (unit, sub) = currency_units(currency.unwrap()).unwrap();
            let plain = word.replace(',', "");
            let mut pairs: Vec<((u64, String), &str)> = plain
                .split('.')
                .zip([unit, sub])
                .map(|(n, u)| (number(n), u))
                .collect();
            if pairs.len() > 1 {
                if pairs[1].0.0 == 0 {
                    pairs.truncate(1);
                } else if pairs[0].0.0 == 0 {
                    pairs.remove(0);
                }
            }
            for (i, ((n, words), u)) in pairs.iter().enumerate() {
                if i > 0 {
                    result.push(self.lookup("and", None, None, None)?);
                }
                extend(&mut result, words);
                result.push(if *n != 1 && *u != "pence" {
                    self.stem_s(&format!("{u}s"), None, None, None)?
                } else {
                    self.lookup(u, None, None, None)?
                });
            }
        } else {
            let words = if is_digit(&word) {
                num(&word)
            } else if !word.contains('.') {
                let n = word.replace(',', "");
                if !is_digit(&n) {
                    return None;
                }
                if ORDINALS.contains(&suffix.as_str()) {
                    ordinalize(&num(&n))
                } else {
                    num(&n)
                }
            } else {
                let w = word.replace(',', "");
                let (int, frac) = w.split_once('.').unwrap();
                if int.is_empty() {
                    format!(
                        "point {}",
                        frac.chars()
                            .map(|c| num(&c.to_string()))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                } else {
                    let frac = frac.trim_end_matches('0');
                    let frac = if frac.is_empty() { "0" } else { frac };
                    format!(
                        "{} point {}",
                        num(int),
                        frac.chars()
                            .map(|c| num(&c.to_string()))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                }
            };
            extend(&mut result, &words);
        }
        if result.is_empty() {
            return None;
        }
        let joined = result.join(" ");
        match suffix.as_str() {
            "s" | "'s" => Some(Self::suffix_s(&joined)),
            "ed" | "'d" => Self::suffix_ed(&joined),
            "ing" => Self::suffix_ing(&joined),
            _ => Some(joined),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Ctx, Lexicon, Tok};

    fn number(lexicon: &Lexicon, text: &str) -> Option<String> {
        lexicon.call(&Tok::new(text, "CD", ""), Ctx::default())
    }

    /// Expected phonemes are Python misaki 0.9.4's for the same text, before
    /// `g2p`'s final ɾ -> T.
    #[test]
    fn reads_numbers_past_a_trillion() {
        let lexicon = Lexicon::load();
        assert_eq!(
            number(&lexicon, "1234567890123456").as_deref(),
            Some(
                "wˈʌn kwɑdɹˈɪljən tˈu hˈʌndɹəd θˈɜɹɾi fˈɔɹ tɹˈɪljən fˈIv hˈʌndɹəd sˈɪksti sˈɛvən \
                 bˈɪljən ˈAt hˈʌndɹəd nˈIndi mˈɪljᵊn wˈʌn hˈʌndɹəd twˈɛnti θɹˈi θˈWzᵊnd \
                 fˈɔɹ hˈʌndɹəd fˈɪfti sˈɪks"
            )
        );
    }

    #[test]
    fn reads_numbers_past_u64_digit_by_digit() {
        let lexicon = Lexicon::load();
        let nines = vec!["nˈIn"; 23].join(" ");
        let big = "99999999999999999999999";
        assert_eq!(number(&lexicon, big), Some(nines.clone()));
        let dollars = |n: &str| {
            let mut tk = Tok::new(n, "CD", "");
            tk.currency = Some("$".into());
            lexicon.call(&tk, Ctx::default()).unwrap()
        };
        let unit = dollars("5").replacen("fˈIv ", "", 1);
        assert_eq!(dollars(big), format!("{nines} {unit}"));
        let ordinal = |n: &str| number(&lexicon, &format!("{n}th")).unwrap();
        let ninth = ordinal("9");
        assert_eq!(
            ordinal(big),
            format!("{} {ninth}", vec!["nˈIn"; 22].join(" "))
        );
        assert_eq!(
            number(&lexicon, &format!("{big},000")),
            number(&lexicon, &format!("{big}000"))
        );
    }

    #[test]
    fn reads_non_ascii_digits() {
        let lexicon = Lexicon::load();
        assert_eq!(number(&lexicon, "٣").as_deref(), Some("θɹˈi"));
        let cases = [
            ("१२", "12"),
            ("٠٩", "09"),
            ("᥇", "1"),
            ("𑛙", "9"),
            ("❸", "3"),
            ("፪", "2"),
            ("⓿", "0"),
        ];
        for (digits, ascii) in cases {
            assert_eq!(
                number(&lexicon, digits),
                number(&lexicon, ascii),
                "{digits}"
            );
        }
    }
}
