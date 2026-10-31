//! Engines without a model. `FakeEngine` is what `OMATALK_TEST_FAKE_ENGINE=1`
//! selects for the black-box tests in `tests/`. `RecordingEngine` records
//! each call for the actor tests.

#[cfg(test)]
use std::sync::{Arc, Mutex};

#[cfg(test)]
use super::SpeechError;
use super::{Audio, Delivery, Engine, StopToken, Utterance};

/// The level a Voiceover's pieces carry. A Live piece is silence, so a test
/// reading the rendered WAV can tell which Delivery the CLI asked for.
pub const VOICEOVER_LEVEL: f32 = 0.25;

/// Splits on sentence ends like the old chunker and yields 0.1 s per piece,
/// so multi-chunk Utterances still exercise one player per Stream. The piece
/// is silent when Live and at `VOICEOVER_LEVEL` for a Voiceover.
pub struct FakeEngine;

impl Engine for FakeEngine {
    fn speak<'a>(&'a mut self, u: &'a Utterance, _: &'a StopToken) -> Audio<'a> {
        let level = match u.delivery {
            Delivery::Live => 0.0,
            Delivery::Voiceover => VOICEOVER_LEVEL,
        };
        Box::new(pieces(u.text.as_str()).map(move |_| Ok(vec![level; 2400])))
    }
}

fn pieces(text: &str) -> impl Iterator<Item = &str> {
    text.split_inclusive(['.', '!', '?'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
}

/// One entry per piece: the text and the bindings the engine saw.
#[cfg(test)]
pub type Call = (String, String, f32);

/// Records bindings per piece. `before` runs ahead of each piece with its
/// index, so a test can hold synthesis (and edit config.toml mid-Utterance)
/// or wait for a side effect; `fail_at` fails that piece after recording it.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct RecordingEngine {
    pub calls: Arc<Mutex<Vec<Call>>>,
    pub before: Option<Arc<dyn Fn(usize) + Send + Sync>>,
    pub fail_at: Option<usize>,
}

#[cfg(test)]
impl Engine for RecordingEngine {
    /// Like Kokoro's guarded ORT run, a piece held in `before` while `stop`
    /// fires ends the audio.
    fn speak<'a>(&'a mut self, u: &'a Utterance, stop: &'a StopToken) -> Audio<'a> {
        Box::new(
            pieces(u.text.as_str())
                .enumerate()
                .map_while(move |(i, piece)| {
                    if let Some(before) = &self.before {
                        before(i);
                    }
                    if stop.is_fired() {
                        return None;
                    }
                    let call = (piece.to_owned(), u.voice.to_string(), u.speed.get());
                    self.calls.lock().unwrap().push(call);
                    Some(if self.fail_at == Some(i) {
                        Err(SpeechError("boom".into()))
                    } else {
                        Ok(vec![0.0; 2400])
                    })
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Speed;
    use crate::speech::Text;
    use crate::voices::VoiceName;

    fn pieces_of(delivery: Delivery) -> Vec<Vec<f32>> {
        let u = Utterance {
            text: Text::new("One. Two.").unwrap(),
            voice: VoiceName::parse("af_heart").unwrap(),
            speed: Speed::NORMAL,
            delivery,
        };
        FakeEngine
            .speak(&u, &StopToken::default())
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn live_pieces_are_silent_and_voiceover_pieces_are_not() {
        assert_eq!(pieces_of(Delivery::Live), vec![vec![0.0; 2400]; 2]);
        assert_eq!(
            pieces_of(Delivery::Voiceover),
            vec![vec![VOICEOVER_LEVEL; 2400]; 2]
        );
    }
}
