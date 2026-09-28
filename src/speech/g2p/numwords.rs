//! Numbers as English words, shared by normalize's rules and misaki's number
//! reader.
use std::num::IntErrorKind;

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALES: [&str; 7] = [
    "",
    "thousand",
    "million",
    "billion",
    "trillion",
    "quadrillion",
    "quintillion",
];
fn below_thousand(n: u64) -> String {
    let (h, r) = (n / 100, n % 100);
    let rest = match r {
        0 => String::new(),
        1..20 => ONES[r as usize].to_string(),
        _ if r % 10 == 0 => TENS[(r / 10) as usize].to_string(),
        _ => format!("{}-{}", TENS[(r / 10) as usize], ONES[(r % 10) as usize]),
    };
    match (h, rest.is_empty()) {
        (0, _) => rest,
        (_, true) => format!("{} hundred", ONES[h as usize]),
        _ => format!("{} hundred {rest}", ONES[h as usize]),
    }
}

pub fn cardinal(mut n: u64) -> String {
    if n == 0 {
        return "zero".into();
    }
    let mut groups = Vec::new();
    let mut scale = 0;
    while n > 0 {
        let group = n % 1000;
        if group > 0 {
            let words = below_thousand(group);
            groups.push(if scale > 0 {
                format!("{words} {}", SCALES[scale])
            } else {
                words
            });
        }
        n /= 1000;
        scale += 1;
    }
    groups.reverse();
    groups.join(" ")
}

/// A digit string's value and words. Past u64 it reads digit by digit, and
/// the value is only good for "not zero, not one".
pub fn number(s: &str) -> (u64, String) {
    match s.parse::<u64>() {
        Err(e) if *e.kind() == IntErrorKind::PosOverflow => (u64::MAX, digits(s)),
        n => {
            let n = n.unwrap_or(0);
            (n, cardinal(n))
        }
    }
}

pub fn ordinal(n: u64) -> String {
    ordinalize(&cardinal(n))
}

pub fn ordinalize(words: &str) -> String {
    let (head, last) = match words.rfind([' ', '-']) {
        Some(i) => words.split_at(i + 1),
        None => ("", words),
    };
    let last = match last {
        "one" => "first".into(),
        "two" => "second".into(),
        "three" => "third".into(),
        "five" => "fifth".into(),
        "eight" => "eighth".into(),
        "nine" => "ninth".into(),
        "twelve" => "twelfth".into(),
        w if w.ends_with('y') => format!("{}ieth", &w[..w.len() - 1]),
        w => format!("{w}th"),
    };
    format!("{head}{last}")
}

pub fn year(n: u64) -> String {
    let (hi, lo) = (n / 100, n % 100);
    match (hi, lo) {
        (_, _) if n % 1000 < 10 && hi % 10 == 0 => cardinal(n),
        (_, 0) => format!("{} hundred", cardinal(hi)),
        (_, 1..10) => format!("{} oh {}", cardinal(hi), cardinal(lo)),
        _ => format!("{} {}", cardinal(hi), cardinal(lo)),
    }
}

pub fn digits(s: &str) -> String {
    s.chars()
        .filter_map(|c| c.to_digit(10))
        .map(|d| ONES[d as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::cardinal;

    #[test]
    fn cardinal_covers_u64() {
        assert_eq!(
            cardinal(u64::MAX),
            "eighteen quintillion four hundred forty-six quadrillion seven hundred forty-four trillion \
             seventy-three billion seven hundred nine million five hundred fifty-one thousand six hundred fifteen"
        );
    }
}
