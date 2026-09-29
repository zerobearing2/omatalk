//! libespeak-ng for misaki's out-of-vocabulary fallback: a port of
//! phonemizer 3.4 `phonemize(text, "en-us", preserve_punctuation=True,
//! with_stress=True, tie="^")`, as misaki's `EspeakFallback` calls it. The
//! library is dlopened, not linked, so only the Daemon ever loads it.
use fancy_regex::Regex;
use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::{LazyLock, Mutex, OnceLock};

use crate::speech::LoadError;

const LIBRARY: &str = "libespeak-ng.so.1";

// speak_lib.h
/// No audio device, no background thread.
const AUDIO_OUTPUT_SYNCHRONOUS: c_int = 0x02;
const PHONEMES_IPA: c_int = 0x02;
const PHONEMES_TIE: c_int = 0x80;
/// The tie character, U+0361, goes in bits 8 and up of the phoneme mode.
const TIE: c_int = 0x0361 << 8;

type TextToPhonemes = unsafe extern "C" fn(*mut *const c_void, c_int, c_int) -> *const c_char;

static MARKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(\s*(?:[;:!?¡¿—…"«»“”(){}\[\]]|(?<![0-9])[,.]|[,.](?![0-9]))+\s*)+"#).unwrap()
});

#[derive(Clone, Copy, PartialEq)]
enum Position {
    Begin,
    Inner,
    End,
    Alone,
}

struct Mark {
    line: usize,
    text: String,
    position: Position,
}

/// espeak-ng keeps its state in C globals, so the process has one instance and
/// every call goes through its mutex. Initialized on first use and kept.
pub struct Espeak {
    text_to_phonemes: TextToPhonemes,
    _library: Library,
}

static ESPEAK: OnceLock<Result<Mutex<Espeak>, String>> = OnceLock::new();

pub fn espeak() -> Result<&'static Mutex<Espeak>, LoadError> {
    ESPEAK
        .get_or_init(|| Espeak::open().map(Mutex::new))
        .as_ref()
        .map_err(|e| LoadError(e.clone()))
}

impl Espeak {
    fn open() -> Result<Espeak, String> {
        let fail = |what: &dyn std::fmt::Display| format!("{LIBRARY}: {what}");
        // SAFETY: the symbol types match speak_lib.h, and `_library` keeps
        // the code mapped for as long as the pointers live.
        unsafe {
            let library = Library::new(LIBRARY).map_err(|e| fail(&e))?;
            let initialize = *library
                .get::<unsafe extern "C" fn(c_int, c_int, *const c_char, c_int) -> c_int>(
                    b"espeak_Initialize\0",
                )
                .map_err(|e| fail(&e))?;
            let set_voice = *library
                .get::<unsafe extern "C" fn(*const c_char) -> c_int>(b"espeak_SetVoiceByName\0")
                .map_err(|e| fail(&e))?;
            let text_to_phonemes = *library
                .get::<TextToPhonemes>(b"espeak_TextToPhonemes\0")
                .map_err(|e| fail(&e))?;
            if initialize(AUDIO_OUTPUT_SYNCHRONOUS, 0, std::ptr::null(), 0) <= 0 {
                return Err(fail(&"espeak_Initialize failed"));
            }
            if set_voice(c"en-us".as_ptr()) != 0 {
                return Err(fail(&"no en-us voice"));
            }
            Ok(Espeak {
                text_to_phonemes,
                _library: library,
            })
        }
    }

    fn text_to_phonemes(&self, text: &str) -> String {
        let Ok(text) = CString::new(text) else {
            return String::new();
        };
        let mut ptr = text.as_ptr() as *const c_void;
        let mode = PHONEMES_IPA | PHONEMES_TIE | TIE;
        let mut parts = Vec::new();
        while !ptr.is_null() {
            let out = unsafe { (self.text_to_phonemes)(&mut ptr, 1, mode) };
            if !out.is_null() {
                let s = unsafe { CStr::from_ptr(out) }
                    .to_string_lossy()
                    .into_owned();
                if !s.is_empty() {
                    parts.push(s);
                }
            }
        }
        parts.join(" ")
    }

    fn phonemize_chunk(&self, chunk: &str) -> String {
        let line = self.text_to_phonemes(chunk);
        let line = line.trim().replace('\n', " ").replace("  ", " ");
        let line = collapse_underscores(&line).replace("_ ", " ");
        if line.is_empty() {
            return String::new();
        }
        line.split(' ')
            .map(|word| {
                let word = word.trim();
                let word = word.replace('\u{0361}', "^");
                format!("{word} ")
            })
            .collect()
    }

    pub fn phonemize_tied(&self, text: &str) -> String {
        let lines: Vec<&str> = text
            .trim_matches('\n')
            .split('\n')
            .filter(|line| !line.trim().is_empty())
            .collect();
        let mut chunks = Vec::new();
        let mut marks = Vec::new();
        for (num, line) in lines.iter().enumerate() {
            let (c, m) = preserve_line(line, num);
            chunks.extend(c);
            marks.extend(m);
        }
        chunks.retain(|c| !c.is_empty());
        let phonemized = chunks.iter().map(|c| self.phonemize_chunk(c)).collect();
        restore(phonemized, marks).join("\n")
    }
}

fn collapse_underscores(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '_' && out.ends_with('_') {
            continue;
        }
        out.push(ch);
    }
    out
}

fn preserve_line(line: &str, num: usize) -> (Vec<String>, Vec<Mark>) {
    let found: Vec<fancy_regex::Match> = MARKS.find_iter(line).map(|m| m.unwrap()).collect();
    let matches: Vec<&str> = found.iter().map(|m| m.as_str()).collect();
    if matches.is_empty() {
        return (vec![line.to_string()], vec![]);
    }
    if matches.len() == 1 && matches[0] == line {
        let mark = Mark {
            line: num,
            text: line.to_string(),
            position: Position::Alone,
        };
        return (vec![], vec![mark]);
    }
    let last = matches.len() - 1;
    let marks: Vec<Mark> = matches
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let position = if i == 0 && line.starts_with(m) {
                Position::Begin
            } else if i == last && line.ends_with(m) {
                Position::End
            } else {
                Position::Inner
            };
            Mark {
                line: num,
                text: m.to_string(),
                position,
            }
        })
        .collect();
    // phonemizer splits on the first occurrence of the mark's text, not at
    // the match position: "It costs $4.99." splits at the decimal point.
    let mut rest = line.to_string();
    let mut chunks = Vec::new();
    for mark in &marks {
        match rest.split_once(mark.text.as_str()) {
            Some((prefix, suffix)) => {
                chunks.push(prefix.to_string());
                rest = suffix.to_string();
            }
            None => {
                chunks.push(rest);
                rest = String::new();
            }
        }
    }
    chunks.push(rest);
    (chunks, marks)
}

fn restore(mut text: Vec<String>, mut marks: Vec<Mark>) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 0;
    text.reverse();
    marks.reverse();
    while !text.is_empty() || !marks.is_empty() {
        if marks.is_empty() {
            while let Some(mut line) = text.pop() {
                if !line.ends_with(' ') {
                    line.push(' ');
                }
                out.push(line);
            }
        } else if text.is_empty() {
            out.push(marks.iter().rev().map(|m| m.text.as_str()).collect());
            marks.clear();
        } else if marks.last().unwrap().line == pos {
            let mark = marks.pop().unwrap();
            let m = mark.text;
            let head = text.last_mut().unwrap();
            if head.ends_with(' ') {
                head.pop();
            }
            let sep = if m.ends_with(' ') { "" } else { " " };
            match mark.position {
                Position::Begin => *head = format!("{m}{head}"),
                Position::End => {
                    let head = text.pop().unwrap();
                    out.push(format!("{head}{m}{sep}"));
                    pos += 1;
                }
                Position::Alone => {
                    out.push(format!("{m}{sep}"));
                    pos += 1;
                }
                Position::Inner => {
                    if text.len() == 1 {
                        text[0].push_str(&m);
                    } else {
                        let first = text.pop().unwrap();
                        let next = text.last_mut().unwrap();
                        *next = format!("{first}{m}{next}");
                    }
                }
            }
        } else {
            out.push(text.pop().unwrap());
            pos += 1;
        }
    }
    out
}
