//! Text to PCM. The deep module: callers hand over an `Utterance` and a
//! `StopToken` and receive 24 kHz mono f32 batches in order. Normalization,
//! G2P, batching, the first-audio ramp, ORT, trim, and pause padding are all
//! behind `Engine::speak`.

pub mod batch;
pub mod fake;
pub mod g2p;
pub mod kokoro;

use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::config::Speed;
use crate::voices::VoiceName;

pub const SAMPLE_RATE: u32 = 24_000;

/// Trimmed, non-empty text. An empty Source is `None`, never `Text("")`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text(String);

impl Text {
    pub fn new(raw: &str) -> Option<Text> {
        let t = raw.trim();
        (!t.is_empty()).then(|| Text(t.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One unit of speech with its bindings fixed at start. Immutable: a running
/// Utterance cannot observe a later config edit because it owns its copy.
#[derive(Clone, Debug)]
pub struct Utterance {
    pub text: Text,
    pub voice: VoiceName,
    pub speed: Speed,
}

/// Each batch's samples (trimmed, pause-padded), synthesized as pulled.
pub type Audio<'a> = Box<dyn Iterator<Item = Result<Vec<f32>, SpeechError>> + 'a>;

/// The seam between the Daemon and a synthesizer. One implementation runs at
/// a time, owned by the synth thread, so `&mut self` needs no locking.
pub trait Engine: Send {
    /// `u`'s audio in order. The caller stops pulling when it no longer
    /// wants PCM; work in flight when `stop` fires registers its abort with
    /// `stop.guard` and ends the iterator.
    fn speak<'a>(&'a mut self, u: &'a Utterance, stop: &'a StopToken) -> Audio<'a>;

    /// Called once when the Daemon goes quiet after speaking: release memory
    /// the last Utterance grew.
    fn rest(&mut self) {}
}

/// Display is the text after `error: ` in the notify, e.g. `unknown voice af_nope`.
#[derive(Debug)]
pub struct SpeechError(pub String);

impl fmt::Display for SpeechError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Load failure at Daemon start (missing model, no libonnxruntime, no
/// libespeak-ng). The Daemon prints it and exits non-zero; systemd retries.
#[derive(Debug)]
pub struct LoadError(pub String);

/// Builds the engine the Daemon will own. The fake skips every native
/// library and data file.
pub fn load_engine(models: &Path, fake: bool) -> Result<Box<dyn Engine>, LoadError> {
    if fake {
        return Ok(Box::new(fake::FakeEngine));
    }
    let mut engine = kokoro::Kokoro::load(models)?;
    engine.warm();
    Ok(Box::new(engine))
}

/// Cancellation for one Utterance, shared by the actor (fires), the synth
/// thread (checks, registers aborts), and the playback thread (checks).
/// Firing is idempotent and never blocks.
#[derive(Clone, Default)]
pub struct StopToken(Arc<StopInner>);

#[cfg(test)]
mod tests {
    use super::StopToken;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn fire_during_guarded_work_calls_abort_once() {
        let token = StopToken::default();
        let aborts = Arc::new(AtomicUsize::new(0));
        let seen = aborts.clone();
        let inner = token.clone();
        let ran = token.guard(
            move || {
                seen.fetch_add(1, Ordering::SeqCst);
            },
            || {
                inner.fire();
                inner.fire();
            },
        );
        assert_eq!(ran, Some(()));
        assert_eq!(aborts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn guard_after_fire_skips_work() {
        let token = StopToken::default();
        token.fire();
        assert_eq!(token.guard(|| {}, || 1), None);
    }
}

#[derive(Default)]
struct StopInner {
    fired: AtomicBool,
    /// The abort for work in flight right now, e.g. ORT `RunOptions::terminate`.
    abort: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl StopToken {
    pub fn fire(&self) {
        if self.0.fired.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(abort) = self.0.abort.lock().unwrap().take() {
            abort();
        }
    }

    pub fn is_fired(&self) -> bool {
        self.0.fired.load(Ordering::Acquire)
    }

    /// Runs `work` with `abort` armed: if the token fires while `work` runs,
    /// `abort` is called once from the firing thread. If the token already
    /// fired, `work` is skipped and `None` is returned.
    pub fn guard<T>(
        &self,
        abort: impl Fn() + Send + 'static,
        work: impl FnOnce() -> T,
    ) -> Option<T> {
        {
            // `fire` sets the flag before taking this lock, so checking it
            // here means a fire either sees our abort or we see the flag.
            let mut slot = self.0.abort.lock().unwrap();
            if self.is_fired() {
                return None;
            }
            *slot = Some(Box::new(abort));
        }
        let out = work();
        self.0.abort.lock().unwrap().take();
        Some(out)
    }
}
