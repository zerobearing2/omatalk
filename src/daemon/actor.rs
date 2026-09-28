//! The actor: the single owner of `Phase` and the follower list.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{Receiver, Sender};

use crate::config::Config;
use crate::exec;
use crate::protocol::State;
use crate::sink::WakeLead;
use crate::speech::{Text, Utterance};
use crate::stream::{Gen, Outcome, Stream, StreamIo, Synth};

/// Everything the actor can be told. Replies travel on one-shot channels so
/// the connection thread, not the actor, writes to the client.
pub(super) enum Command {
    Speak {
        press: Press,
        /// Read at this press, voice override applied; binds a new Utterance.
        config: Box<Config>,
        reply: Sender<SpeakOutcome>,
    },
    Stop {
        reply: Sender<()>,
    },
    Status {
        reply: Sender<State>,
    },
    /// Write the current state and register, as one step.
    Follow(UnixStream),
    /// From a playback thread; ignored unless `generation` is the current Stream's.
    StreamEnded {
        generation: Gen,
        outcome: Outcome,
    },
}

/// What a press carried, resolved off the actor. The Clipboard is read in a
/// second round trip, only when the actor answers `NeedClipboard`, so a press
/// that stops speech never waits on a clipboard read.
#[derive(Debug)]
pub(super) enum Press {
    /// Non-blank inline text, else the Selection; `None` when both are empty.
    First(Option<Text>),
    /// The follow-up after `NeedClipboard`.
    Clipboard(Option<Text>),
}

#[derive(Debug, PartialEq)]
pub(super) enum SpeakOutcome {
    Ok,
    /// Idle and the Selection empty: read the Clipboard and press again.
    NeedClipboard,
    /// Idle and every Source empty: the connection thread notifies.
    NothingToRead,
}

#[derive(Debug, PartialEq)]
enum Decision {
    Start(Text),
    Stop,
    NeedClipboard,
    NothingToRead,
    /// A Clipboard follow-up that lost a race with another press.
    Keep,
}

/// The Source and Interrupt rules from CONTEXT.md, as one pure function.
///
/// - Inline text, else the Selection, resolves the text.
/// - Resolved text equal to what is playing: stop.
/// - Nothing resolved while speaking: stop (never falls to the Clipboard).
/// - Nothing resolved while not speaking: ask for the Clipboard.
/// - Clipboard text while idle: start; empty: nothing to read. If another
///   press started speech between the two round trips, the follow-up keeps it.
/// - Anything else: start (an Interrupt if something is playing).
fn decide(playing: Option<&Text>, press: Press) -> Decision {
    match (press, playing) {
        (Press::First(Some(text)), Some(now)) if &text == now => Decision::Stop,
        (Press::First(Some(text)), _) => Decision::Start(text),
        (Press::First(None), Some(_)) => Decision::Stop,
        (Press::First(None), None) => Decision::NeedClipboard,
        (Press::Clipboard(_), Some(_)) => Decision::Keep,
        (Press::Clipboard(Some(text)), None) => Decision::Start(text),
        (Press::Clipboard(None), None) => Decision::NothingToRead,
    }
}

/// `State` is derived from `Phase`; there is no second field to keep in sync.
enum Phase {
    Idle,
    Speaking {
        text: Text,
        stream: Stream,
    },
    /// Persists until `stop` or a successful start.
    Error,
}

impl Phase {
    fn state(&self) -> State {
        match self {
            Phase::Idle => State::Idle,
            Phase::Speaking { .. } => State::Speaking,
            Phase::Error => State::Error,
        }
    }

    fn playing(&self) -> Option<&Text> {
        match self {
            Phase::Speaking { text, .. } => Some(text),
            _ => None,
        }
    }
}

pub(super) struct Actor {
    phase: Phase,
    last_gen: u64,
    followers: Followers,
    synth: Synth,
    /// For playback threads to report `StreamEnded`.
    commands: Sender<Command>,
}

impl Actor {
    pub(super) fn new(synth: Synth, commands: Sender<Command>) -> Actor {
        Actor {
            phase: Phase::Idle,
            last_gen: 0,
            followers: Followers::default(),
            synth,
            commands,
        }
    }

    /// The only place state lines are emitted: once per change, after the
    /// command that caused it. Interrupt is Speaking -> Speaking: no line.
    pub(super) fn run(mut self, inbox: Receiver<Command>) {
        for cmd in inbox {
            let before = self.phase.state();
            self.handle(cmd);
            let after = self.phase.state();
            if after != before {
                self.followers.broadcast(after);
            }
        }
    }

    fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Speak {
                press,
                config,
                reply,
            } => {
                let outcome = self.speak(press, *config);
                let _ = reply.send(outcome);
            }
            Command::Stop { reply } => {
                // Dropping a Speaking phase drops its Stream, which interrupts it.
                self.phase = Phase::Idle;
                let _ = reply.send(());
            }
            Command::Status { reply } => {
                let _ = reply.send(self.phase.state());
            }
            Command::Follow(stream) => self.followers.add(stream, self.phase.state()),
            Command::StreamEnded {
                generation,
                outcome,
            } => {
                let current = matches!(&self.phase, Phase::Speaking { stream, .. } if stream.generation() == generation);
                if current {
                    self.phase = match outcome {
                        Outcome::Finished | Outcome::Stopped => Phase::Idle,
                        Outcome::Failed(_) => Phase::Error,
                    };
                }
            }
        }
    }

    fn speak(&mut self, press: Press, config: Config) -> SpeakOutcome {
        match decide(self.phase.playing(), press) {
            Decision::Stop => self.phase = Phase::Idle,
            Decision::Keep => {}
            Decision::NeedClipboard => return SpeakOutcome::NeedClipboard,
            Decision::NothingToRead => return SpeakOutcome::NothingToRead,
            Decision::Start(text) => {
                self.last_gen += 1;
                let generation = Gen(self.last_gen);
                let Config {
                    voice,
                    speed,
                    player,
                    notify,
                    sink_probe,
                    wake_lead,
                    ..
                } = config;
                let utterance = Utterance {
                    text: text.clone(),
                    voice,
                    speed,
                };
                let io = StreamIo {
                    player,
                    wake: WakeLead {
                        probe: sink_probe,
                        lead: wake_lead,
                    },
                };
                let commands = self.commands.clone();
                let stream = Stream::start(
                    utterance,
                    io,
                    generation,
                    &self.synth,
                    // Notify before the report, so the error is shown by the
                    // time state flips to `error`.
                    move |generation, outcome| {
                        if let Outcome::Failed(msg) = &outcome {
                            eprintln!("error: {msg}");
                            exec::notify(&notify, &format!("error: {msg}"));
                        }
                        let _ = commands.send(Command::StreamEnded {
                            generation,
                            outcome,
                        });
                    },
                );
                // The old Stream (if any) drops here: Interrupt.
                self.phase = Phase::Speaking { text, stream };
            }
        }
        SpeakOutcome::Ok
    }
}

/// `follow` connections. Non-blocking sockets: a state line is at most 9
/// bytes, so `WouldBlock` means the client left the whole socket buffer
/// unread. It is dropped; the plugin reconnects in 1 s and gets a fresh
/// snapshot.
#[derive(Default)]
struct Followers(Vec<UnixStream>);

impl Followers {
    /// Prunes closed followers (EOF on read), then writes `state` to the new
    /// one and keeps it if the write succeeded. Runs on the actor, so no
    /// transition can land between the snapshot and registration.
    fn add(&mut self, mut stream: UnixStream, state: State) {
        self.0.retain(|s| !closed(s));
        if stream.set_nonblocking(true).is_ok() && write_line(&mut stream, state) {
            self.0.push(stream);
        }
    }

    /// One line to every follower; any write error or short write drops that follower.
    fn broadcast(&mut self, state: State) {
        self.0.retain_mut(|s| write_line(s, state));
    }
}

fn write_line(stream: &mut UnixStream, state: State) -> bool {
    let line = format!("{}\n", state.wire());
    matches!(stream.write(line.as_bytes()), Ok(n) if n == line.len())
}

/// The client hung up. Followers send nothing after `follow`, so the bytes
/// this read may consume were never going to be read.
fn closed(mut stream: &UnixStream) -> bool {
    match stream.read(&mut [0; 64]) {
        Ok(n) => n == 0,
        Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn t(s: &str) -> Option<Text> {
        Text::new(s)
    }

    #[test]
    fn same_selection_while_speaking_stops() {
        let now = t("Sticky.").unwrap();
        assert_eq!(
            decide(Some(&now), Press::First(t("Sticky."))),
            Decision::Stop
        );
    }

    #[test]
    fn empty_selection_while_speaking_stops_without_clipboard() {
        let now = t("Playing.").unwrap();
        assert_eq!(decide(Some(&now), Press::First(None)), Decision::Stop);
    }

    #[test]
    fn empty_selection_while_idle_asks_for_clipboard() {
        assert_eq!(decide(None, Press::First(None)), Decision::NeedClipboard);
        assert_eq!(
            decide(None, Press::Clipboard(t("Clip."))),
            Decision::Start(t("Clip.").unwrap())
        );
        assert_eq!(
            decide(None, Press::Clipboard(None)),
            Decision::NothingToRead
        );
    }

    #[test]
    fn clipboard_follow_up_keeps_speech_another_press_started() {
        let now = t("Other press.").unwrap();
        assert_eq!(
            decide(Some(&now), Press::Clipboard(t("Clip."))),
            Decision::Keep
        );
    }

    #[test]
    fn new_text_interrupts() {
        let now = t("Old.").unwrap();
        assert_eq!(
            decide(Some(&now), Press::First(t("New."))),
            Decision::Start(t("New.").unwrap())
        );
    }

    #[test]
    fn a_follower_that_never_reads_is_dropped_without_blocking() {
        let (server, _client) = UnixStream::pair().unwrap();
        let mut followers = Followers::default();
        followers.add(server, State::Idle);
        let started = Instant::now();
        let mut writes = 0;
        while !followers.0.is_empty() {
            followers.broadcast(if writes % 2 == 0 {
                State::Speaking
            } else {
                State::Idle
            });
            writes += 1;
            assert!(writes < 1_000_000, "follower never dropped");
        }
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "broadcast blocked: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn closed_followers_are_pruned_when_another_registers() {
        let mut followers = Followers::default();
        let (gone, client) = UnixStream::pair().unwrap();
        followers.add(gone, State::Idle);
        drop(client);
        let (live, _client) = UnixStream::pair().unwrap();
        followers.add(live, State::Idle);
        assert_eq!(followers.0.len(), 1);
    }
}
