//! Phoneme strings to model-sized batches, and the ramp that decides how
//! big each batch may be. Pure: no ORT, no clock, fully unit-tested.
//!
//! Split/pack is the prototype's `split_phonemes` (atoms at sentence, then
//! clause, then word bounds; balanced packing to <= 510). The ramp is new.

use std::collections::VecDeque;

/// Kokoro's context limit (tokens between the two pad tokens).
pub const MAX_PHONEMES: usize = 510;

/// First batch cap: about 4-5 words, one ORT run of ~250-450 ms.
pub const FIRST_CAP: usize = 32;

/// Kokoro fp32 through this engine on a Ryzen 7840HS, onnxruntime 1.29
/// (`examples/speak.rs --rate` over README paragraphs and prose). Synthesis is
/// ~200 ms + 7 ms/phoneme for short batches and ~11 ms/phoneme for long ones;
/// the ramp takes the slower slope so it errs toward smaller batches.
pub const SYNTH_FLOOR_MS: f32 = 190.0;
pub const SYNTH_MS_PER_PHONEME: f32 = 11.0;
/// Audio produced per phoneme at speed 1.0, trimmed and without pauses
/// (measured 58-61).
pub const AUDIO_MS_PER_PHONEME: f32 = 58.0;

/// One model call's input plus the silence to append after its audio.
#[derive(Debug, PartialEq)]
pub struct Batch {
    pub phonemes: String,
    /// 250 ms after `.!?…`, 100 ms after `,;:`, 0 otherwise; always 0 on the
    /// last batch of the Utterance.
    pub pause_ms: u32,
}

/// Cap for the next batch. Invariant: predicted synth time of the next batch
/// is at most the predicted audio duration of the previous one, so playback
/// never runs dry between batches (on the reference CPU). Monotone: never
/// shrinks, and saturates at `MAX_PHONEMES`.
#[derive(Clone, Copy, Debug)]
pub struct Ramp {
    cap: usize,
    speed: f32,
}

impl Ramp {
    pub fn new(speed: f32) -> Ramp {
        Ramp {
            cap: FIRST_CAP,
            speed,
        }
    }

    pub fn cap(&self) -> usize {
        self.cap
    }

    /// Advance after emitting a batch of `len` phonemes.
    pub fn after(self, len: usize) -> Ramp {
        let audio_ms = len as f32 * AUDIO_MS_PER_PHONEME / self.speed;
        let affordable = ((audio_ms - SYNTH_FLOOR_MS) / SYNTH_MS_PER_PHONEME).max(0.0) as usize;
        Ramp {
            cap: affordable.clamp(self.cap, MAX_PHONEMES),
            speed: self.speed,
        }
    }
}

/// Lazily turns phonemized lines into batches. Pulls a new line only when the
/// pending phonemes cannot fill the current cap, so batch 1 is ready after
/// G2P of line 1 alone.
pub struct Batches<I: Iterator<Item = String>> {
    lines: I,
    exhausted: bool,
    /// Phonemes pulled but not yet batched, lines joined by one space.
    pending: String,
    /// Full-size batches already packed, waiting to be emitted.
    packed: VecDeque<String>,
    ramp: Ramp,
}

impl<I: Iterator<Item = String>> Batches<I> {
    pub fn new(lines: I, speed: f32) -> Self {
        Batches {
            lines,
            exhausted: false,
            pending: String::new(),
            packed: VecDeque::new(),
            ramp: Ramp::new(speed),
        }
    }

    /// Pulls lines until `pending` is longer than `cap` or the lines run out.
    fn fill(&mut self, cap: usize) {
        while !self.exhausted && len(&self.pending) <= cap {
            match self.lines.next() {
                None => self.exhausted = true,
                Some(line) => {
                    let line = line.trim();
                    if !line.is_empty() {
                        if !self.pending.is_empty() {
                            self.pending.push(' ');
                        }
                        self.pending.push_str(line);
                    }
                }
            }
        }
    }

    /// Below full size: the longest prefix under the cap that ends a
    /// sentence, else a clause, else a word.
    fn ramped(&mut self) -> Option<String> {
        let cap = self.ramp.cap();
        self.fill(cap);
        if self.pending.is_empty() {
            return None;
        }
        let at = cut(&self.pending, cap);
        let head = self.pending[..at].trim_end().to_owned();
        self.pending = self.pending[at..].trim_start().to_owned();
        Some(head)
    }

    /// At full size: the prototype's balanced packing over what is pending.
    /// The last packed batch goes back to `pending` while lines remain, so it
    /// can share a batch with the next line.
    fn pack(&mut self) -> Option<String> {
        self.fill(MAX_PHONEMES);
        let mut batches = split_phonemes(&std::mem::take(&mut self.pending));
        if !self.exhausted {
            self.pending = batches.pop().unwrap_or_default();
        }
        self.packed.extend(batches);
        self.packed.pop_front()
    }
}

impl<I: Iterator<Item = String>> Iterator for Batches<I> {
    type Item = Batch;

    fn next(&mut self) -> Option<Batch> {
        let phonemes = match self.packed.pop_front() {
            Some(batch) => batch,
            None if self.ramp.cap() < MAX_PHONEMES => self.ramped()?,
            None => self.pack()?,
        };
        let last = self.exhausted && self.pending.is_empty() && self.packed.is_empty();
        self.ramp = self.ramp.after(len(&phonemes));
        let pause_ms = if last { 0 } else { pause_after(&phonemes) };
        Some(Batch { phonemes, pause_ms })
    }
}

fn len(phonemes: &str) -> usize {
    phonemes.chars().count()
}

/// Where a batch may end, strongest first: after a sentence end, after a
/// clause end, after any word.
#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum Bound {
    Sentence,
    Clause,
    Word,
}

/// The bound a whitespace run makes after `c`.
fn bound(c: Option<char>) -> Bound {
    match c {
        Some('.' | '!' | '?' | '…') => Bound::Sentence,
        Some(',' | ';' | ':') => Bound::Clause,
        _ => Bound::Word,
    }
}

fn pause_after(batch: &str) -> u32 {
    match bound(batch.trim_end().chars().last()) {
        Bound::Sentence => 250,
        Bound::Clause => 100,
        Bound::Word => 0,
    }
}

/// Byte offset ending the longest prefix of at most `cap` chars that stops
/// before a space: after a sentence end if one fits, else a clause end, else
/// any word. With no space in reach, a hard cut at `cap` chars.
fn cut(phonemes: &str, cap: usize) -> usize {
    // The last cut found at each bound, indexed by `Bound`.
    let mut cuts = [None; 3];
    let mut prev = None;
    for (count, (at, c)) in phonemes.char_indices().enumerate() {
        if count > cap {
            break;
        }
        if c.is_whitespace() && count > 0 {
            cuts[bound(prev) as usize] = Some(at);
        }
        prev = Some(c);
    }
    if len(phonemes) <= cap {
        return phonemes.len();
    }
    cuts.into_iter().flatten().next().unwrap_or_else(|| {
        phonemes
            .char_indices()
            .nth(cap)
            .map_or(phonemes.len(), |(at, _)| at)
    })
}

/// Splits `s` at every whitespace run whose bound is at least as strong as
/// `level`, dropping the run.
fn split_at(s: &str, level: Bound) -> Vec<&str> {
    let mut pieces = Vec::new();
    let (mut last, mut prev, mut run) = (0, None, None);
    for (at, c) in s.char_indices() {
        if !c.is_whitespace() {
            if let Some(start) = run.take() {
                pieces.push(&s[last..start]);
                last = at;
            }
        } else if !prev.is_some_and(char::is_whitespace) && bound(prev) <= level {
            run = Some(at);
        }
        prev = Some(c);
    }
    if let Some(start) = run {
        pieces.push(&s[last..start]);
        last = s.len();
    }
    pieces.push(&s[last..]);
    pieces
}

/// Splits at the strongest bound at or below `level` that splits at all.
fn atoms(phonemes: &str, max: usize, level: Bound, out: &mut Vec<String>) {
    if phonemes.chars().count() <= max {
        if !phonemes.is_empty() {
            out.push(phonemes.to_string());
        }
        return;
    }
    for bound in [Bound::Sentence, Bound::Clause, Bound::Word] {
        if bound < level {
            continue;
        }
        let pieces = split_at(phonemes, bound);
        if pieces.len() > 1 {
            for piece in pieces {
                atoms(piece.trim(), max, bound, out);
            }
            return;
        }
    }
    let chars: Vec<char> = phonemes.chars().collect();
    for slice in chars.chunks(max) {
        out.push(slice.iter().collect());
    }
}

fn pack(lengths: &[usize], limit: usize) -> Vec<(usize, usize)> {
    let mut batches = Vec::new();
    let (mut start, mut size) = (0, 0);
    for (index, &length) in lengths.iter().enumerate() {
        let candidate = if index == start {
            length
        } else {
            size + 1 + length
        };
        if candidate > limit && index > start {
            batches.push((start, index));
            start = index;
            size = length;
        } else {
            size = candidate;
        }
    }
    if !lengths.is_empty() {
        batches.push((start, lengths.len()));
    }
    batches
}

/// The prototype's packing: as few batches as fit in `MAX_PHONEMES`, with the
/// smallest limit that keeps that count, so batches come out even.
fn split_phonemes(phonemes: &str) -> Vec<String> {
    let mut parts = Vec::new();
    atoms(phonemes.trim(), MAX_PHONEMES, Bound::Sentence, &mut parts);
    if parts.is_empty() {
        return parts;
    }
    let lengths: Vec<usize> = parts.iter().map(|a| a.chars().count()).collect();
    let fewest = pack(&lengths, MAX_PHONEMES).len();
    let (mut low, mut high) = (*lengths.iter().max().unwrap(), MAX_PHONEMES);
    while low < high {
        let middle = (low + high) / 2;
        if pack(&lengths, middle).len() <= fewest {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    pack(&lengths, low)
        .into_iter()
        .map(|(s, e)| parts[s..e].join(" "))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// fancy-regex's lookbehind split gave up on lines this long.
    #[test]
    fn splits_a_huge_line_without_marks_at_words() {
        let line = "abcde ".repeat(200_000);
        let parts = split_phonemes(&line);
        assert!(parts.iter().all(|p| p.len() <= MAX_PHONEMES));
        assert_eq!(parts.join(" "), line.trim_end());
    }

    fn batches(lines: &[&str], speed: f32) -> Vec<Batch> {
        Batches::new(lines.iter().map(|l| l.to_string()), speed).collect()
    }

    /// `n` words of `w` chars; every `per`th word ends a sentence.
    fn prose(words: usize, w: usize, per: usize) -> String {
        (1..=words)
            .map(|i| "a".repeat(w - 1) + if i % per == 0 { "." } else { "b" })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn ramp_grows_from_first_cap_to_max_at_speed_one() {
        let ramp = Ramp::new(1.0);
        assert_eq!(ramp.cap(), FIRST_CAP);
        let second = ramp.after(FIRST_CAP);
        assert_eq!(second.cap(), 151);
        assert_eq!(second.after(second.cap()).cap(), MAX_PHONEMES);
    }

    #[test]
    fn ramp_never_shrinks() {
        let ramp = Ramp::new(1.0).after(FIRST_CAP);
        assert_eq!(ramp.after(1).cap(), 151);
        assert_eq!(Ramp::new(1.0).after(0).cap(), FIRST_CAP);
        assert_eq!(ramp.after(10_000).after(1).cap(), MAX_PHONEMES);
    }

    #[test]
    fn faster_speech_ramps_slower() {
        assert_eq!(Ramp::new(2.0).after(FIRST_CAP).cap(), 67);
        assert!(Ramp::new(0.5).after(FIRST_CAP).cap() > 151);
    }

    #[test]
    fn short_text_is_one_batch_without_trailing_pause() {
        assert_eq!(
            batches(&["hˈI."], 1.0),
            [Batch {
                phonemes: "hˈI.".into(),
                pause_ms: 0
            }]
        );
        assert!(batches(&[], 1.0).is_empty());
        assert!(batches(&["", "  "], 1.0).is_empty());
    }

    #[test]
    fn batches_fit_and_rejoin_to_the_input() {
        let lines = [
            prose(40, 6, 5),
            "ʃˈɔɹt.".into(),
            prose(300, 5, 7),
            prose(3, 4, 9),
        ];
        let all: Vec<&str> = lines.iter().map(String::as_str).collect();
        let out = batches(&all, 1.0);
        assert!(
            out.iter()
                .all(|b| len(&b.phonemes) <= MAX_PHONEMES && !b.phonemes.is_empty())
        );
        let joined: Vec<&str> = out.iter().map(|b| b.phonemes.as_str()).collect();
        assert_eq!(joined.join(" "), all.join(" "));
        assert_eq!(out.last().unwrap().pause_ms, 0);
    }

    #[test]
    fn first_batch_of_a_huge_line_is_capped() {
        let line = prose(500, 6, 4);
        assert!(len(&line) >= 3000);
        let out = batches(&[&line], 1.0);
        assert!(len(&out[0].phonemes) <= FIRST_CAP);
        let caps: Vec<usize> = out.iter().map(|b| len(&b.phonemes)).collect();
        assert!(caps[1] > FIRST_CAP && caps[1] <= 151, "{caps:?}");
    }

    #[test]
    fn first_cut_prefers_sentence_then_clause_then_word() {
        let first = |line: &str| batches(&[line], 1.0).remove(0);
        assert_eq!(
            first("wˈʌn tˈu. θɹˈi fˈɔɹ, fˈIv sˈɪks sˈɛvən"),
            Batch {
                phonemes: "wˈʌn tˈu.".into(),
                pause_ms: 250
            }
        );
        assert_eq!(
            first("wˈʌn tˈu θɹˈi, fˈɔɹ fˈIv sˈɪks sˈɛvən ˈAt"),
            Batch {
                phonemes: "wˈʌn tˈu θɹˈi,".into(),
                pause_ms: 100
            }
        );
        assert_eq!(
            first("wˈʌn tˈu θɹˈi fˈɔɹ fˈIv sˈɪks sˈɛvən ˈAt").phonemes,
            "wˈʌn tˈu θɹˈi fˈɔɹ fˈIv sˈɪks"
        );
        assert_eq!(first(&"x".repeat(40)).phonemes, "x".repeat(FIRST_CAP));
    }

    #[test]
    fn batch_one_needs_only_line_one() {
        let pulled = Cell::new(0);
        let lines = [prose(20, 5, 3), prose(20, 5, 3), prose(20, 5, 3)];
        let mut out = Batches::new(
            lines
                .iter()
                .inspect(|_| pulled.set(pulled.get() + 1))
                .cloned(),
            1.0,
        );
        out.next().unwrap();
        assert_eq!(pulled.get(), 1);
    }

    #[test]
    fn full_size_batches_match_the_prototype_packing() {
        // Four 200-char sentences: greedy packing at 510 gives 2 batches, and
        // the balanced search keeps 2 at the smallest limit, 401.
        let line = prose(4, 200, 1);
        let packed = split_phonemes(&line);
        assert_eq!(
            packed.iter().map(|b| len(b)).collect::<Vec<_>>(),
            [401, 401]
        );

        let mut full = Batches::new([line.clone()].into_iter(), 1.0);
        full.ramp.cap = MAX_PHONEMES;
        let out: Vec<String> = full.map(|b| b.phonemes).collect();
        assert_eq!(out, packed);
    }

    #[test]
    fn after_the_ramp_a_single_line_packs_like_the_prototype() {
        let line = prose(400, 6, 6);
        let out = batches(&[&line], 1.0);
        // Batches 1 and 2 are ramped (<= 32, <= 151); the ramp is then at 510.
        let head = format!("{} {}", out[0].phonemes, out[1].phonemes);
        assert!(line.starts_with(&head));
        let tail: Vec<String> = out[2..].iter().map(|b| b.phonemes.clone()).collect();
        assert_eq!(tail, split_phonemes(&line[head.len()..]));
    }

    #[test]
    fn pauses_follow_the_last_mark() {
        assert_eq!(pause_after("a."), 250);
        assert_eq!(pause_after("a…"), 250);
        assert_eq!(pause_after("a;"), 100);
        assert_eq!(pause_after("a"), 0);
    }
}
