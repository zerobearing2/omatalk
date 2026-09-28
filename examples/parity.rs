//! The G2P parity lever:
//!
//!   cargo run --release --example parity -- e2e tools/parity/sample.jsonl
//!   cargo run --release --example parity -- tok|tag|e2e <corpus.jsonl> [--min N]
//!
//! Each corpus row is `{"text", "tokens", "phonemes"}` from
//! `tools/parity/dump_misaki.py` (Python spaCy + misaki). `tok` diffs the
//! tokenizer against spaCy's tokens; `tag` tags spaCy's tokens and runs misaki
//! on them; `e2e` runs raw text through the whole G2P. Prints the counts,
//! writes diffs to `target/parity-<mode>-diffs.txt`, and exits 1 when fewer
//! than `--min` lines match.

use std::fmt::Write;
use std::process::ExitCode;
use std::time::Instant;

use omatalk::speech::g2p::{G2p, espeak, misaki, tagger::Tagger, tokenize, tokenize::Tokenizer};
use serde_json::Value;

const UNK: &str = "❓";

struct Row {
    text: String,
    /// spaCy's (text, tag, whitespace) per token.
    tokens: Vec<(String, String, String)>,
    phonemes: String,
}

fn rows(path: &str) -> Vec<Row> {
    let corpus = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    corpus
        .lines()
        .map(|line| {
            let v: Value = serde_json::from_str(line).expect("corpus row");
            let s = |v: &Value| v.as_str().unwrap_or_default().to_owned();
            let tokens = v["tokens"].as_array().map_or(Vec::new(), |ts| {
                ts.iter().map(|t| (s(&t[0]), s(&t[1]), s(&t[2]))).collect()
            });
            Row {
                text: s(&v["text"]),
                tokens,
                phonemes: s(&v["phonemes"]),
            }
        })
        .collect()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mode, path) = match &args[..] {
        [mode, path, ..] => (mode.as_str(), path.as_str()),
        _ => {
            eprintln!("usage: parity tok|tag|e2e <corpus.jsonl> [--min N]");
            return ExitCode::from(2);
        }
    };
    let min: usize = match &args[2..] {
        [flag, n] if flag == "--min" => n.parse().expect("--min N"),
        _ => 0,
    };
    let rows = rows(path);
    let t = Instant::now();
    let (same, diffs) = match mode {
        "tok" => tok(&rows),
        "tag" => tag(&rows),
        "e2e" => e2e(&rows),
        _ => {
            eprintln!("unknown mode {mode}");
            return ExitCode::from(2);
        }
    };
    let total = rows.len();
    println!(
        "{same}/{total} lines identical ({:.3}%) in {:.1}s",
        same as f64 * 100.0 / total as f64,
        t.elapsed().as_secs_f64()
    );
    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/target");
    let _ = std::fs::create_dir_all(out);
    let file = format!("{out}/parity-{mode}-diffs.txt");
    std::fs::write(&file, diffs).expect("write diffs");
    println!("diffs: {file}");
    if same < min {
        eprintln!("below the floor of {min}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn tok(rows: &[Row]) -> (usize, String) {
    let tokenizer = Tokenizer::load().expect("tokenizer");
    let (mut same, mut want_tokens, mut hit_tokens) = (0, 0, 0);
    let mut diffs = String::new();
    for row in rows {
        let want: Vec<(String, String)> = row
            .tokens
            .iter()
            .map(|(t, _, ws)| (t.clone(), ws.clone()))
            .collect();
        let got = tokenizer.tokenize(row.text.trim_start_matches(tokenize::is_space));
        want_tokens += want.len();
        hit_tokens += matched(&want, &got);
        if got == want {
            same += 1;
        } else {
            let fmt = |ts: &[(String, String)]| {
                ts.iter()
                    .map(|(t, w)| format!("{t:?}{}", if w.is_empty() { "" } else { "_" }))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            writeln!(
                diffs,
                "{:?}\n  want: {}\n  got:  {}",
                row.text,
                fmt(&want),
                fmt(&got)
            )
            .unwrap();
        }
    }
    println!("{hit_tokens}/{want_tokens} reference tokens matched");
    (same, diffs)
}

/// Reference tokens whose byte span and whitespace both appear in the output.
fn matched(want: &[(String, String)], got: &[(String, String)]) -> usize {
    let spans = |ts: &[(String, String)]| {
        let mut at = 0;
        ts.iter()
            .map(|(t, w)| {
                let s = (at, at + t.len(), w.clone());
                at += t.len() + w.len();
                s
            })
            .collect::<std::collections::HashSet<_>>()
    };
    let g = spans(got);
    spans(want).iter().filter(|s| g.contains(*s)).count()
}

fn tag(rows: &[Row]) -> (usize, String) {
    let tagger = Tagger::load().expect("tagger");
    let lexicon = misaki::Lexicon::load().expect("lexicon");
    let espeak = espeak::espeak().expect("espeak").lock().unwrap();
    let (mut same, mut tokens, mut right) = (0, 0, 0);
    let mut diffs = String::new();
    for row in rows {
        let words: Vec<&str> = row.tokens.iter().map(|t| t.0.as_str()).collect();
        let spaces: Vec<bool> = row.tokens.iter().map(|t| !t.2.is_empty()).collect();
        let tags = tagger.tag(&words, &spaces);
        tokens += words.len();
        for ((word, want, _), got) in row.tokens.iter().zip(&tags) {
            if want == got {
                right += 1;
            } else {
                writeln!(diffs, "tag {word} {want}->{got} | {}", row.text).unwrap();
            }
        }
        let toks = row
            .tokens
            .iter()
            .zip(&tags)
            .map(|((w, _, ws), tag)| misaki::Tok::new(w, tag, ws))
            .collect();
        let got = misaki::g2p(&lexicon, toks, UNK, |text| espeak.phonemize_tied(text));
        if got == row.phonemes {
            same += 1;
        } else {
            writeln!(diffs, "{}\n  want {}\n  got  {got}", row.text, row.phonemes).unwrap();
        }
    }
    println!("tag accuracy {right}/{tokens}; phoneme lines below use our tags");
    (same, diffs)
}

fn e2e(rows: &[Row]) -> (usize, String) {
    let g2p = G2p::load().expect("g2p");
    let mut same = 0;
    let mut diffs = String::new();
    for row in rows {
        let got = g2p.phonemize(&row.text, UNK);
        if got == row.phonemes {
            same += 1;
        } else {
            writeln!(diffs, "{}\n  want {}\n  got  {got}", row.text, row.phonemes).unwrap();
        }
    }
    (same, diffs)
}
