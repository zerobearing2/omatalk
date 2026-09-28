//! Kokoro-82M v1.0 fp32 through `ort` against the system onnxruntime
//! (`load-dynamic`). Port of the prototype's `Kokoro` + `main` loop.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use ort::session::{RunOptions, Session};
use ort::value::Tensor;

use super::batch::Batches;
use super::g2p::G2p;
use super::{Audio, Engine, LoadError, SAMPLE_RATE, SpeechError, StopToken, Utterance};
use crate::voices::{VOICES_FILE, VoiceName};

pub const MODEL_FILE: &str = "kokoro-v1.0.onnx";

/// libonnxruntime candidates, first that loads wins. `ORT_DYLIB_PATH` (ort's
/// own variable) overrides them all. Arch's `onnxruntime-cpu` installs both
/// names in /usr/lib, which the dynamic loader searches.
const ORT_LIBRARIES: [&str; 2] = ["libonnxruntime.so.1", "libonnxruntime.so"];

const STYLE_WIDTH: usize = 256;

pub struct Kokoro {
    session: Session,
    /// The graph's token input name (`tokens` or `input_ids` by export).
    input: String,
    /// Phoneme char -> token id, from the embedded `data/vocab.json`.
    vocab: HashMap<char, i64>,
    styles: Styles,
    g2p: G2p,
}

impl Kokoro {
    /// Loads the ORT session and the G2P data on parallel threads (~400 ms
    /// and ~160 ms measured); the total is the session load. Errors name the
    /// missing file or library.
    pub fn load(models: &Path) -> Result<Kokoro, LoadError> {
        load_onnxruntime()?;
        let model = models.join(MODEL_FILE);
        let archive = models.join(VOICES_FILE);
        if !archive.is_file() {
            return Err(LoadError(format!("{}: not found", archive.display())));
        }
        let (session, g2p) = thread::scope(|s| {
            let g2p = s.spawn(G2p::load);
            let session = Session::builder()
                .and_then(|mut b| b.commit_from_file(&model))
                .map_err(|e| LoadError(format!("{}: {e}", model.display())));
            let g2p = g2p
                .join()
                .unwrap_or_else(|_| Err(LoadError("G2P data failed to load".into())));
            (session, g2p)
        });
        let session = session?;
        let input = session.inputs()[0].name().to_owned();
        Ok(Kokoro {
            session,
            input,
            vocab: vocab()?,
            styles: Styles::new(archive),
            g2p: g2p?,
        })
    }

    /// One throwaway pass through G2P (normalize, tagger, lexicon, espeak for
    /// the unknown "Omatalk") and ORT, so the first real press is not cold.
    pub fn warm(&mut self) {
        self.tiny_run(Arena::Keep);
    }

    fn tiny_run(&mut self, arena: Arena) {
        let phonemes = known(&self.vocab, &self.g2p.line("Warm up, Omatalk."));
        let tokens = tokens(&self.vocab, &phonemes);
        let style = [0.0; STYLE_WIDTH];
        let _ = infer(
            &mut self.session,
            &self.input,
            &tokens,
            &style,
            1.0,
            &StopToken::default(),
            arena,
        );
    }
}

/// ORT's CPU arena grows to the largest batch and keeps it. Shrinking after
/// every run costs first audio (a third of presses +150 ms), so only the
/// idle `rest` shrinks it: a long Utterance's ~700 MB goes back to the system.
#[derive(Clone, Copy)]
enum Arena {
    Keep,
    Shrink,
}

impl Engine for Kokoro {
    fn speak<'a>(&'a mut self, u: &'a Utterance, stop: &'a StopToken) -> Audio<'a> {
        let Kokoro {
            session,
            input,
            vocab,
            styles,
            g2p,
        } = self;
        let vocab = &*vocab;
        let lines = phonemize(u.text.as_str(), stop, move |line| {
            known(vocab, &g2p.line(line))
        });
        Box::new(Batches::new(lines, u.speed.get()).map_while(move |batch| {
            let tokens = tokens(vocab, &batch.phonemes);
            let style = match styles.row(&u.voice, tokens.len()) {
                Ok(style) => style,
                Err(e) => return Some(Err(e)),
            };
            let audio = infer(
                session,
                input,
                &tokens,
                style,
                u.speed.get(),
                stop,
                Arena::Keep,
            )?;
            Some(audio.map(|audio| {
                let mut audio = trim(&audio).to_vec();
                audio.resize(
                    audio.len() + (batch.pause_ms * SAMPLE_RATE / 1000) as usize,
                    0.0,
                );
                audio
            }))
        }))
    }

    /// ORT shrinks the arena only at the end of a run that asks for it.
    fn rest(&mut self) {
        self.tiny_run(Arena::Shrink);
    }
}

/// Phonemizes line by line on demand, so batch 1 waits for G2P of line 1
/// only, and pulls no more lines once `stop` fires.
fn phonemize<'a>(
    text: &'a str,
    stop: &'a StopToken,
    mut g2p: impl FnMut(&str) -> String + 'a,
) -> impl Iterator<Item = String> + 'a {
    text.lines()
        .map_while(move |line| (!stop.is_fired()).then(|| g2p(line)))
}

/// Loads libonnxruntime before any other `ort` call, which would otherwise
/// panic on a missing library. `ort::init_from` must run at most once: after
/// a failure its internal OnceLock reads as initialized, so candidates are
/// probed with a plain dlopen first.
fn load_onnxruntime() -> Result<(), LoadError> {
    let candidates: Vec<String> = match std::env::var("ORT_DYLIB_PATH") {
        Ok(path) if !path.is_empty() => vec![path],
        _ => ORT_LIBRARIES.map(String::from).to_vec(),
    };
    // SAFETY: loading libonnxruntime runs no initializers with preconditions.
    let found = candidates
        .iter()
        .find(|path| unsafe { libloading::Library::new(path.as_str()) }.is_ok());
    let Some(path) = found else {
        return Err(LoadError(format!(
            "onnxruntime: none of {} loads",
            candidates.join(", ")
        )));
    };
    ort::init_from(path)
        .map_err(|e| LoadError(format!("onnxruntime: {e}")))?
        .commit();
    Ok(())
}

fn vocab() -> Result<HashMap<char, i64>, LoadError> {
    let config: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/data/vocab.json"
    )))
    .map_err(|e| LoadError(format!("vocab.json: {e}")))?;
    let entries = config["vocab"]
        .as_object()
        .ok_or_else(|| LoadError("vocab.json: no vocab".into()))?;
    entries
        .iter()
        .map(|(k, v)| match (k.chars().next(), v.as_i64()) {
            (Some(c), Some(id)) => Ok((c, id)),
            _ => Err(LoadError(format!("vocab.json: bad entry {k}"))),
        })
        .collect()
}

/// Drops what the model has no token for and collapses whitespace.
fn known(vocab: &HashMap<char, i64>, phonemes: &str) -> String {
    let known: String = phonemes.chars().filter(|c| vocab.contains_key(c)).collect();
    known.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tokens(vocab: &HashMap<char, i64>, phonemes: &str) -> Vec<i64> {
    phonemes
        .chars()
        .filter_map(|c| vocab.get(&c).copied())
        .collect()
}

/// Style vectors per voice (510 rows x 256), read from the npz archive on
/// first use and kept, so only voices actually spoken are resident (~0.5 MB
/// each).
struct Styles {
    archive: PathBuf,
    loaded: HashMap<VoiceName, Vec<f32>>,
}

impl Styles {
    fn new(archive: PathBuf) -> Styles {
        Styles {
            archive,
            loaded: HashMap::new(),
        }
    }

    /// Row `min(tokens, rows) - 1` of the voice's style matrix.
    fn row(&mut self, voice: &VoiceName, tokens: usize) -> Result<&[f32], SpeechError> {
        if !self.loaded.contains_key(voice) {
            let styles = read_styles(&self.archive, voice)?;
            self.loaded.insert(voice.clone(), styles);
        }
        let styles = &self.loaded[voice];
        let row = tokens.clamp(1, styles.len() / STYLE_WIDTH) - 1;
        Ok(&styles[row * STYLE_WIDTH..(row + 1) * STYLE_WIDTH])
    }
}

fn read_styles(archive: &Path, voice: &VoiceName) -> Result<Vec<f32>, SpeechError> {
    let broken = |e: std::io::Error| SpeechError(format!("{}: {e}", archive.display()));
    let mut npz = npyz::npz::NpzArchive::open(archive).map_err(broken)?;
    let array = npz.by_name(voice.as_str()).map_err(broken)?;
    let array = array.ok_or_else(|| SpeechError(format!("unknown voice {voice}")))?;
    let styles: Vec<f32> = array.into_vec().map_err(broken)?;
    if styles.is_empty() || !styles.len().is_multiple_of(STYLE_WIDTH) {
        return Err(SpeechError(format!("voice {voice}: bad style matrix")));
    }
    Ok(styles)
}

/// One ORT run under `stop`: the run's `RunOptions::terminate` is armed via
/// `stop.guard`, so a fire aborts it mid-graph. `None` when stopped.
fn infer(
    session: &mut Session,
    input: &str,
    tokens: &[i64],
    style: &[f32],
    speed: f32,
    stop: &StopToken,
    arena: Arena,
) -> Option<Result<Vec<f32>, SpeechError>> {
    let failed = |e: ort::Error| SpeechError(format!("synthesis failed: {e}"));
    let options = RunOptions::new().and_then(|mut options| {
        if let Arena::Shrink = arena {
            options.set("memory.enable_memory_arena_shrinkage", "cpu:0")?;
        }
        Ok(Arc::new(options))
    });
    let options = match options {
        Ok(options) => options,
        Err(e) => return Some(Err(failed(e))),
    };
    let abort = {
        let options = options.clone();
        move || {
            let _ = options.terminate();
        }
    };
    let result = stop.guard(abort, || -> Result<Vec<f32>, ort::Error> {
        let mut ids = Vec::with_capacity(tokens.len() + 2);
        ids.push(0);
        ids.extend_from_slice(tokens);
        ids.push(0);
        let n = ids.len();
        let inputs = ort::inputs![
            input => Tensor::from_array(([1usize, n], ids))?,
            "style" => Tensor::from_array(([1usize, STYLE_WIDTH], style.to_vec()))?,
            "speed" => Tensor::from_array(([1usize], vec![speed]))?,
        ];
        let outputs = session.run_with_options(inputs, &options)?;
        let (_, audio) = outputs[0].try_extract_tensor::<f32>()?;
        Ok(audio.to_vec())
    })?;
    if stop.is_fired() {
        return None;
    }
    Some(result.map_err(failed))
}

/// librosa.effects.trim(top_db=60, frame_length=2048, hop_length=512), as in
/// the prototype and kokoro-onnx. Returns the loud span of `audio`.
pub fn trim(audio: &[f32]) -> &[f32] {
    const FRAME: usize = 2048;
    const HOP: usize = 512;
    let mut padded = vec![0f32; FRAME / 2];
    padded.extend_from_slice(audio);
    padded.extend(std::iter::repeat_n(0f32, FRAME / 2));
    let frames = 1 + (padded.len() - FRAME) / HOP;
    let power: Vec<f32> = (0..frames)
        .map(|i| {
            padded[i * HOP..i * HOP + FRAME]
                .iter()
                .map(|x| x * x)
                .sum::<f32>()
                / FRAME as f32
        })
        .collect();
    let peak = power.iter().cloned().fold(0f32, f32::max).max(1e-10);
    let loud: Vec<usize> = (0..frames)
        .filter(|&i| 10.0 * (power[i].max(1e-10) / peak).log10() > -60.0)
        .collect();
    match (loud.first(), loud.last()) {
        (Some(&first), Some(&last)) => &audio[first * HOP..audio.len().min((last + 1) * HOP)],
        _ => &audio[0..0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Speed;
    use std::time::{Duration, Instant};

    #[test]
    fn stop_ends_phonemizing_at_the_next_line() {
        let stop = StopToken::default();
        let mut seen = Vec::new();
        let out: Vec<String> = phonemize("one\ntwo\nthree", &stop, |line| {
            seen.push(line.to_owned());
            stop.fire();
            line.to_uppercase()
        })
        .collect();
        assert_eq!(out, ["ONE"]);
        assert_eq!(seen, ["one"]);
    }

    fn tone(n: usize, amplitude: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 24_000.0).sin())
            .collect()
    }

    // Expected spans from kokoro_onnx.trim (the librosa port) on the same signals.
    #[test]
    fn trim_matches_librosa() {
        let mut audio = vec![0.0; 24_000];
        audio.extend(tone(12_000, 0.5));
        audio.extend(vec![0.0; 24_000]);
        let kept = trim(&audio);
        let start = kept.as_ptr() as usize - audio.as_ptr() as usize;
        assert_eq!((start / 4, start / 4 + kept.len()), (23_040, 37_376));

        let mut quiet_tail = tone(12_000, 0.5);
        quiet_tail.extend(tone(12_000, 0.5e-4));
        assert_eq!(trim(&quiet_tail).len(), 13_312);
    }

    #[test]
    fn trim_keeps_all_of_uniform_silence() {
        assert_eq!(trim(&[0.0; 5000]).len(), 5000);
        assert!(trim(&[]).is_empty());
    }

    fn utterance(text: &str) -> Utterance {
        Utterance {
            text: crate::speech::Text::new(text).unwrap(),
            voice: VoiceName::parse("af_heart").unwrap(),
            speed: Speed::parse("1.0").unwrap(),
        }
    }

    /// Needs `OMATALK_MODELS` (kokoro-v1.0.onnx fp32 + voices-v1.0.bin),
    /// libespeak-ng, and libonnxruntime (or `ORT_DYLIB_PATH`).
    #[test]
    #[ignore = "needs the real model; run with --ignored"]
    fn real_model_speaks_and_stops() {
        let models = std::env::var("OMATALK_MODELS").expect("set OMATALK_MODELS");
        let mut kokoro = Kokoro::load(Path::new(&models)).expect("load");
        kokoro.warm();

        let u = utterance(
            "Hello from Rust. This sentence is long enough to need a second batch, or so we hope.",
        );
        let batches: Vec<Vec<f32>> = kokoro
            .speak(&u, &StopToken::default())
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(batches.len() >= 2, "{} batches", batches.len());
        let seconds = batches.iter().map(Vec::len).sum::<usize>() as f32 / SAMPLE_RATE as f32;
        assert!((3.0..12.0).contains(&seconds), "{seconds}s of audio");
        assert!(batches.iter().flatten().any(|s| s.abs() > 0.05));

        let unknown = Utterance {
            voice: VoiceName::parse("af_nope").unwrap(),
            ..u.clone()
        };
        let err = kokoro
            .speak(&unknown, &StopToken::default())
            .collect::<Result<Vec<_>, _>>()
            .unwrap_err();
        assert_eq!(err.to_string(), "unknown voice af_nope");

        // Fired 600 ms in: batch 1 (<= 32 phonemes) is out and batch 2 is
        // mid-run, so speak must return promptly with nothing more emitted.
        let long =
            utterance(&"The quick brown fox jumps over the lazy dog, again and again. ".repeat(20));
        let stop = StopToken::default();
        let (mut outs, mut fired_at) = (0, None);
        let returned = thread::scope(|s| {
            let firing = s.spawn(|| {
                thread::sleep(Duration::from_millis(600));
                let at = Instant::now();
                stop.fire();
                at
            });
            for batch in kokoro.speak(&long, &stop) {
                batch.unwrap();
                outs += 1;
            }
            let returned = Instant::now();
            fired_at = Some(firing.join().unwrap());
            returned
        });
        let fired_at = fired_at.unwrap();
        assert!(returned > fired_at, "speak finished before the stop");
        assert_eq!(outs, 1);
        assert!(
            returned - fired_at < Duration::from_millis(50),
            "stop took {:?}",
            returned - fired_at
        );
    }
}
