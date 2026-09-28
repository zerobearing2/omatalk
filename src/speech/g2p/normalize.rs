//! Spells out numbers, money, times, dates, URLs and file names before
//! phonemization, following misaki's conventions (the G2P Kokoro was trained
//! with): four-digit numbers read as years, decimals digit by digit.
use super::numwords::{cardinal, digits, number, ordinal, ordinalize, year};
use fancy_regex::Regex;
type Captures<'t> = fancy_regex::Captures<'t, str>;
use std::sync::LazyLock;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn integer(s: &str) -> String {
    let plain: String = s.chars().filter(|c| *c != ',').collect();
    match plain.parse::<u64>() {
        Ok(n) if plain.len() <= 15 => cardinal(n),
        _ => digits(&plain),
    }
}

fn decimal(int: &str, frac: &str) -> String {
    format!("{} point {}", integer(int), digits(frac))
}

fn money(caps: &Captures) -> String {
    let (unit, subunit) = match &caps[1] {
        "£" => ("pound", "penny"),
        "€" => ("euro", "cent"),
        _ => ("dollar", "cent"),
    };
    let plural = |word: &str, n: u64| match (word, n) {
        (w, 1) => w.to_string(),
        ("penny", _) => "pence".into(),
        (w, _) => format!("{w}s"),
    };
    let whole = &caps[2];
    let frac = caps.get(3).map(|m| m.as_str());
    if let Some(scale) = caps.get(4) {
        let amount = match frac {
            Some(f) => decimal(whole, f),
            None => integer(whole),
        };
        return format!("{amount} {} {}s", scale.as_str(), unit);
    }
    let n = number(&whole.replace(',', "")).0;
    let (cents, cent_words) = frac.map_or((0, String::new()), |f| number(&format!("{f:0<2}")));
    match (n, cents) {
        (0, c) if c > 0 => format!("{cent_words} {}", plural(subunit, c)),
        (n, 0) => format!("{} {}", integer(whole), plural(unit, n)),
        (n, c) => format!(
            "{} {} and {cent_words} {}",
            integer(whole),
            plural(unit, n),
            plural(subunit, c)
        ),
    }
}

fn time(caps: &Captures) -> String {
    let (h, m): (u64, u64) = (caps[1].parse().unwrap(), caps[2].parse().unwrap());
    if h > 24 || m > 59 {
        return caps[0].to_string();
    }
    let hour = cardinal(h);
    match m {
        0 if caps.get(3).is_some() => hour,
        0 => format!("{hour} o'clock"),
        1..10 => format!("{hour} oh {}", cardinal(m)),
        _ => format!("{hour} {}", cardinal(m)),
    }
}

fn spoken_address(s: &str) -> String {
    s.replace('.', " dot ")
        .replace('/', " slash ")
        .replace('@', " at ")
        .replace(['?', '=', '&', '#', '_', '-', ':'], " ")
}

type Spell = fn(&Captures<'_>) -> String;
type Rule = (&'static str, Spell);

// Order matters: each rule sees the text the earlier rules left.
const RULES: &[Rule] = &[
    (
        r"[\x{1F000}-\x{1FAFF}\x{2600}-\x{27BF}\x{FE0F}\x{200D}]+\s?",
        |_| String::new(),
    ),
    (
        r"\b(Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sept?|Oct|Nov|Dec|vs|etc|approx|dept|St|Ave|Blvd)\.(?=\s|$)",
        |c| {
            let word = match &c[1] {
                "Jan" => "January",
                "Feb" => "February",
                "Mar" => "March",
                "Apr" => "April",
                "Jun" => "June",
                "Jul" => "July",
                "Aug" => "August",
                "Sep" | "Sept" => "September",
                "Oct" => "October",
                "Nov" => "November",
                "Dec" => "December",
                "vs" => "versus",
                "etc" => "et cetera",
                "approx" => "approximately",
                "dept" => "department",
                "St" => "Street",
                "Ave" => "Avenue",
                _ => "Boulevard",
            };
            word.to_string()
        },
    ),
    (r"\be\.g\.", |_| "for example".into()),
    (r"\bi\.e\.", |_| "that is".into()),
    (r"https?://(\S+?)(?=[.,;:!?)]*(?:\s|$))", |c| {
        spoken_address(&c[1])
    }),
    (r"\b[\w.+-]+@[\w-]+(?:\.[\w-]+)+\b", |c| {
        spoken_address(&c[0])
    }),
    (r"\b(\d{4})-(\d{1,2})-(\d{1,2})\b", |c| {
        let (y, m, d): (u64, usize, u64) = (
            c[1].parse().unwrap(),
            c[2].parse().unwrap(),
            c[3].parse().unwrap(),
        );
        match m {
            1..=12 => format!("{} {}, {}", MONTHS[m - 1], ordinal(d), year(y)),
            _ => c[0].to_string(),
        }
    }),
    (r"\b(\d{3})-(\d{3})-(\d{4})\b", |c| {
        format!("{}, {}, {}", digits(&c[1]), digits(&c[2]), digits(&c[3]))
    }),
    (
        r"([$£€])\s?(\d{1,3}(?:,\d{3})+|\d+)(?:\.(\d+))?(?:\s(thousand|million|billion|trillion)\b)?",
        money,
    ),
    (r"(-?)(\d[\d,]*)(?:\.(\d+))?%", |c| {
        let sign = if c[1].is_empty() { "" } else { "minus " };
        let n = match c.get(3) {
            Some(f) => decimal(&c[2], f.as_str()),
            None => integer(&c[2]),
        };
        format!("{sign}{n} percent")
    }),
    (r"\b(\d{1,2}):(\d{2})\b(\s?[AaPp]\.?[Mm]\b)?", |c| {
        let suffix = c.get(3).map(|m| m.as_str()).unwrap_or("");
        format!("{}{suffix}", time(c))
    }),
    (r"\b[vV]?(\d+(?:\.\d+){2,})\b", |c| {
        c[1].split('.')
            .map(integer)
            .collect::<Vec<_>>()
            .join(" point ")
    }),
    (r"(?<![\w.])(-?)(\d[\d,]*)\.(\d+)\b", |c| {
        let sign = if c[1].is_empty() { "" } else { "minus " };
        format!("{sign}{}", decimal(&c[2], &c[3]))
    }),
    (r"#(\d+)\b", |c| format!("number {}", integer(&c[1]))),
    (r"\b(\d[\d,]*)(?:st|nd|rd|th)\b", |c| {
        ordinalize(&number(&c[1].replace(',', "")).1)
    }),
    (r"(?<![\w,])(\d{4})(?!\w|,\d)", |c| {
        year(c[1].parse().unwrap())
    }),
    (r"(?<![\w.])-(\d)", |c| format!("minus {}", &c[1])),
    (r"\b\d{1,3}(?:,\d{3})+\b|\b\d+\b", |c| integer(&c[0])),
    (r"\b([A-Za-z]\w+)\.([a-z]{2,5})\b(?!\.\w)", |c| {
        format!("{} dot {}", &c[1], &c[2])
    }),
];

static COMPILED: LazyLock<Vec<(Regex, Spell)>> = LazyLock::new(|| {
    RULES
        .iter()
        .map(|(re, f)| (Regex::new(re).unwrap(), *f))
        .collect()
});

pub fn normalize(text: &str) -> String {
    text.split('\n')
        .map(normalize_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_line(text: &str) -> String {
    COMPILED.iter().fold(text.to_string(), |text, (re, f)| {
        re.replace_all(&text, |c: &Captures| f(c)).into_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn numbers_past_a_trillion() {
        assert_eq!(
            normalize("the 1234567890123456th time"),
            "the one quadrillion two hundred thirty-four trillion five hundred sixty-seven billion \
             eight hundred ninety million one hundred twenty-three thousand four hundred fifty-sixth time"
        );
    }

    #[test]
    fn numbers_past_u64_read_digit_by_digit() {
        let nines = "nine ".repeat(22);
        assert_eq!(
            normalize("the 99999999999999999999999th"),
            format!("the {nines}ninth")
        );
        assert_eq!(
            normalize("$99999999999999999999999.50"),
            format!("{nines}nine dollars and fifty cents")
        );
        assert_eq!(
            normalize("$1.12345678901234567890123"),
            "one dollar and one two three four five six seven eight nine zero one two three four \
             five six seven eight nine zero one two three cents"
        );
    }

    #[test]
    fn spoken_forms() {
        let cases = [
            (
                "It costs $4.99.",
                "It costs four dollars and ninety-nine cents.",
            ),
            (
                "Only $1 or $0.50 or £2.",
                "Only one dollar or fifty cents or two pounds.",
            ),
            (
                "$1,299.00 total",
                "one thousand two hundred ninety-nine dollars total",
            ),
            (
                "$1.5 million raised",
                "one point five million dollars raised",
            ),
            (
                "Pi is 3.14159.",
                "Pi is three point one four one five nine.",
            ),
            ("Up 12.5% today", "Up twelve point five percent today"),
            (
                "It was -4.5 degrees",
                "It was minus four point five degrees",
            ),
            (
                "Numbers like 1,234,567 matter",
                "Numbers like one million two hundred thirty-four thousand five hundred sixty-seven matter",
            ),
            (
                "Version 0.5.1 shipped",
                "Version zero point five point one shipped",
            ),
            (
                "At 3:45 p.m. and 06:30 AM",
                "At three forty-five p.m. and six thirty AM",
            ),
            (
                "Meet at 10:00 or 9:05.",
                "Meet at ten o'clock or nine oh five.",
            ),
            (
                "On 2026-09-27.",
                "On September twenty-seventh, twenty twenty-six.",
            ),
            (
                "In 1999, 2008 and 1905.",
                "In nineteen ninety-nine, two thousand eight and nineteen oh five.",
            ),
            (
                "The 1st, 2nd, 23rd and 100th",
                "The first, second, twenty-third and one hundredth",
            ),
            (
                "Call 555-123-4567 now",
                "Call five five five, one two three, four five six seven now",
            ),
            (
                "Bug #28 in main.rs.",
                "Bug number twenty-eight in main dot rs.",
            ),
            (
                "Mail dave@example.com today",
                "Mail dave at example dot com today",
            ),
            (
                "See https://example.com/v1.",
                "See example dot com slash v1.",
            ),
            ("The U.S.A. at 5 p.m.", "The U.S.A. at five p.m."),
            (
                "GPUs vs. CPUs on Sept. 3rd",
                "GPUs versus CPUs on September third",
            ),
            (
                "Baker St. and e.g. apples",
                "Baker Street and for example apples",
            ),
            ("Smile 😀 and wave 👋 now", "Smile and wave now"),
        ];
        for (input, want) in cases {
            assert_eq!(normalize(input), want, "input: {input}");
        }
    }
}
