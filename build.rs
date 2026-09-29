//! Compiles `data/us_{gold,silver}.json` into sorted tables that
//! `src/speech/g2p/misaki/lexicon.rs` binary-searches in place, so the
//! Daemon neither parses nor heap-allocates ~180k entries at startup.
//!
//! Table: u32 count, then count + 1 u32 record offsets, then the records.
//! A record is `key 0x00 body`. A plain body is the phonemes. A tagged body
//! is 0x01 followed by `tag 0x1f phonemes` pairs joined by 0x1e, with 0x02 as
//! the phonemes of a tag that goes to espeak-ng. All integers little-endian.

use std::collections::BTreeMap;
use std::path::Path;
use std::{env, fs};

use serde_json::Value;

fn main() {
    let out = env::var("OUT_DIR").unwrap();
    for name in ["us_gold", "us_silver"] {
        let src = format!("data/{name}.json");
        println!("cargo:rerun-if-changed={src}");
        let json: BTreeMap<String, Value> =
            serde_json::from_str(&fs::read_to_string(&src).unwrap()).unwrap();
        fs::write(
            Path::new(&out).join(format!("{name}.lex")),
            table(&grow(json)),
        )
        .unwrap();
    }
    println!("cargo:rerun-if-changed=build.rs");
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.as_str().to_lowercase().chars())
            .collect(),
        None => String::new(),
    }
}

/// en.py's `grow_dictionary`: lowercase keys also under their capitalized
/// form and capitalized keys under their lowercase one, the JSON's own keys
/// winning (`{**e, **d}`).
fn grow(raw: BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    let mut grown = BTreeMap::new();
    for (k, v) in &raw {
        if k.chars().count() < 2 {
            continue;
        }
        let low = k.to_lowercase();
        if *k == low {
            let cap = capitalize(k);
            if *k != cap {
                grown.insert(cap, v.clone());
            }
        } else if *k == capitalize(&low) {
            grown.insert(low, v.clone());
        }
    }
    grown.extend(raw);
    grown
}

fn table(entries: &BTreeMap<String, Value>) -> Vec<u8> {
    let mut records = Vec::new();
    let mut offsets = Vec::new();
    for (key, value) in entries {
        offsets.push(records.len() as u32);
        records.extend(checked(key).as_bytes());
        records.push(0x00);
        match value {
            Value::String(ps) => records.extend(checked(ps).as_bytes()),
            Value::Object(tags) => {
                assert!(
                    tags.get("DEFAULT").is_some_and(Value::is_string),
                    "{key}: tagged entry has no DEFAULT"
                );
                records.push(0x01);
                for (i, (tag, ps)) in tags.iter().enumerate() {
                    if i > 0 {
                        records.push(0x1e);
                    }
                    records.extend(checked(tag).as_bytes());
                    records.push(0x1f);
                    match ps {
                        Value::String(ps) => records.extend(checked(ps).as_bytes()),
                        Value::Null => records.push(0x02),
                        other => panic!("{key}/{tag}: unexpected {other}"),
                    }
                }
            }
            other => panic!("{key}: unexpected {other}"),
        }
    }
    offsets.push(records.len() as u32);
    let mut out = (entries.len() as u32).to_le_bytes().to_vec();
    for o in offsets {
        out.extend(o.to_le_bytes());
    }
    out.extend(records);
    out
}

fn checked(s: &str) -> &str {
    assert!(
        !s.bytes().any(|b| matches!(b, 0x00..=0x02 | 0x1e | 0x1f)),
        "{s:?} holds a table separator"
    );
    s
}
