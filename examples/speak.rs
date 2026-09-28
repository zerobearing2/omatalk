//! The Kokoro engine lever: load, warm, speak, and time it, without the Daemon.
//!
//!   OMATALK_MODELS=<dir> cargo run --release --example speak -- [flags] "text"   (or text on stdin)
//!
//! Flags: `--voice af_heart`, `--speed 1.0`, `--out file.f32` (raw 24 kHz
//! mono f32), `--stop-after MS` (fire the StopToken mid-speech and time the
//! return), `--rate` (speak each input line, time every batch, and fit the
//! Ramp constants in `speech/batch.rs`: synth floor, synth ms per phoneme,
//! audio ms per phoneme).

use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use omatalk::config::Speed;
use omatalk::speech::batch::{Batch, Batches};
use omatalk::speech::g2p::G2p;
use omatalk::speech::kokoro::Kokoro;
use omatalk::speech::{Engine, SAMPLE_RATE, StopToken, Text, Utterance};
use omatalk::voices::VoiceName;

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut voice, mut speed, mut out, mut stop_after, mut rate) =
        ("af_heart".to_owned(), "1.0".to_owned(), None, None, false);
    let mut words = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--voice" => voice = args.next().expect("--voice NAME"),
            "--speed" => speed = args.next().expect("--speed N"),
            "--out" => out = args.next().map(PathBuf::from),
            "--stop-after" => {
                stop_after = args
                    .next()
                    .map(|n| Duration::from_millis(n.parse().expect("--stop-after MS")))
            }
            "--rate" => rate = true,
            _ => words.push(a),
        }
    }
    let text = if words.is_empty() {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).expect("stdin");
        s
    } else {
        words.join(" ")
    };
    let models = PathBuf::from(std::env::var("OMATALK_MODELS").expect("set OMATALK_MODELS"));

    let t = Instant::now();
    let g2p = G2p::load().expect("g2p");
    eprintln!("g2p alone: load {:.0} ms", ms(t.elapsed()));
    drop(g2p);
    let t = Instant::now();
    let mut kokoro = Kokoro::load(&models).expect("load");
    eprintln!("Kokoro::load (session ∥ g2p): {:.0} ms", ms(t.elapsed()));
    let t = Instant::now();
    kokoro.warm();
    eprintln!("warm: {:.0} ms", ms(t.elapsed()));

    let utterance = |text: &str| Utterance {
        text: Text::new(text).expect("non-empty text"),
        voice: VoiceName::parse(&voice).expect("voice name"),
        speed: Speed::parse(&speed).expect("speed"),
    };
    if rate {
        fit_rate(&mut kokoro, &text, &utterance);
        return;
    }

    let stop = StopToken::default();
    let (mut samples, mut arrivals) = (Vec::new(), Vec::new());
    let start = Instant::now();
    let fired = std::thread::scope(|s| {
        let fired = stop_after.map(|after| {
            let stop = stop.clone();
            s.spawn(move || {
                std::thread::sleep(after);
                stop.fire();
                Instant::now()
            })
        });
        let u = utterance(&text);
        for pcm in kokoro.speak(&u, &stop) {
            let pcm = pcm.expect("speak");
            arrivals.push((start.elapsed(), pcm.len()));
            samples.extend(pcm);
        }
        fired.map(|f| f.join().unwrap())
    });
    let total = start.elapsed();
    let audio = samples.len() as f64 / SAMPLE_RATE as f64;
    // When playback of what arrived so far would run dry, if played at once.
    let mut dry = Duration::ZERO;
    for (i, &(at, n)) in arrivals.iter().enumerate() {
        let len = Duration::from_secs_f64(n as f64 / SAMPLE_RATE as f64);
        let gap = if i == 0 {
            Duration::ZERO
        } else {
            at.saturating_sub(dry)
        };
        eprintln!(
            "batch {}: at {:.0} ms, {:.2} s audio, gap {:.0} ms",
            i + 1,
            ms(at),
            len.as_secs_f64(),
            ms(gap)
        );
        dry = dry.max(at) + len;
    }
    if let Some(&(first, _)) = arrivals.first() {
        eprintln!("first audio: {:.0} ms", ms(first));
    }
    eprintln!(
        "{} batches, {audio:.2} s audio in {:.0} ms, RTF {:.3}",
        arrivals.len(),
        ms(total),
        total.as_secs_f64() / audio.max(1e-9)
    );
    if let Some(fired) = fired {
        eprintln!(
            "stop: speak returned {:.1} ms after fire",
            ms(Instant::now()
                .min(start + total)
                .saturating_duration_since(fired))
        );
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status
        .lines()
        .filter(|l| l.starts_with("VmHWM") || l.starts_with("VmRSS"))
    {
        eprintln!("{}", line.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    if let Some(path) = out {
        let bytes: Vec<u8> = samples.iter().flat_map(|x| x.to_le_bytes()).collect();
        std::fs::write(path, bytes).expect("write --out");
    }
}

/// Speaks each input line, pairs every batch the engine emits with the same
/// batch from a `Batches` replay (for its phoneme count and pause), and fits
/// the Ramp's constants over all batches.
fn fit_rate(kokoro: &mut Kokoro, text: &str, utterance: &dyn Fn(&str) -> Utterance) {
    let g2p = G2p::load().expect("g2p");
    let mut runs = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let u = utterance(line);
        let phonemes = g2p
            .line(line)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let replay: Vec<Batch> = Batches::new(std::iter::once(phonemes), u.speed.get()).collect();
        let mut last = Instant::now();
        for (i, pcm) in kokoro.speak(&u, &StopToken::default()).enumerate() {
            let pcm = pcm.expect("speak");
            let synth = ms(last.elapsed());
            let audio = pcm.len() as f64 * 1000.0 / SAMPLE_RATE as f64 - replay[i].pause_ms as f64;
            runs.push((replay[i].phonemes.chars().count() as f64, synth, audio));
            last = Instant::now();
        }
    }
    let n = runs.len() as f64;
    let (mx, my) = (
        runs.iter().map(|r| r.0).sum::<f64>() / n,
        runs.iter().map(|r| r.1).sum::<f64>() / n,
    );
    let slope = runs.iter().map(|r| (r.0 - mx) * (r.1 - my)).sum::<f64>()
        / runs.iter().map(|r| (r.0 - mx).powi(2)).sum::<f64>();
    for (phonemes, synth, audio) in &runs {
        eprintln!("{phonemes:4} phonemes: synth {synth:5.0} ms, audio {audio:6.0} ms");
    }
    eprintln!(
        "{} batches: synth ≈ {:.0} ms + {:.2} ms/phoneme",
        runs.len(),
        my - slope * mx,
        slope
    );
    eprintln!(
        "audio: {:.1} ms/phoneme",
        runs.iter().map(|r| r.2).sum::<f64>() / runs.iter().map(|r| r.0).sum::<f64>()
    );
}
