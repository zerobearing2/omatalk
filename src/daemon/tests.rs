//! The actor driven through the connection-thread `speak`.

use super::actor::{Actor, Command, Gen};
use super::*;
use crate::protocol::State;
use crate::speech::fake::{Call, RecordingEngine};
use crate::stream::{Outcome, REST_AFTER};
use crate::testutil::{TempDir, fake_commands, fake_env, log_lines, wait_for};
use crate::voices::VoiceName;
use std::collections::BTreeMap;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(10);

/// A real Actor and Synth, driven through the connection-thread
/// `speak`, with the shell fakes from tests/fakes/ as helpers.
struct Harness {
    dir: TempDir,
    config: PathBuf,
    commands: Sender<Command>,
    calls: Arc<Mutex<Vec<Call>>>,
}

impl Harness {
    fn new(engine: RecordingEngine) -> Harness {
        Harness::with(|_| engine)
    }

    /// For an engine that watches files in the harness dir.
    fn with(make: impl FnOnce(&Path) -> RecordingEngine) -> Harness {
        let dir = TempDir::new("actor");
        for file in ["play.log", "notify.log", "capture.txt", "clipboard.txt"] {
            std::fs::write(dir.path().join(file), "").unwrap();
        }
        std::fs::write(dir.path().join("ticks.txt"), "10").unwrap();
        let engine = make(dir.path());
        let (commands, inbox) = channel();
        let calls = engine.calls.clone();
        let actor = Actor::new(Synth::spawn(Box::new(engine), REST_AFTER), commands.clone());
        std::thread::spawn(move || actor.run(inbox));
        let h = Harness {
            config: dir.path().join("config.toml"),
            dir,
            commands,
            calls,
        };
        h.write_config(&[]);
        h
    }

    /// `argv` run with the fakes' env vars pointing into this harness.
    fn fake(&self, program: &str) -> String {
        let env = fake_env(self.dir.path()).map(|(k, v)| format!("{k}={}", v.display()));
        let words: Vec<String> = std::iter::once("env".to_owned())
            .chain(env)
            .chain([program.to_owned()])
            .collect();
        serde_json::to_string(&words).unwrap()
    }

    fn write_config(&self, overrides: &[(&str, String)]) {
        let mut keys = BTreeMap::from([
            ("voice", "\"af_heart\"".to_owned()),
            ("speed", "1.0".to_owned()),
        ]);
        keys.extend(fake_commands(|program| self.fake(program)));
        keys.extend(overrides.iter().map(|(k, v)| (*k, v.clone())));
        let body: String = keys.iter().map(|(k, v)| format!("{k} = {v}\n")).collect();
        std::fs::write(&self.config, body).unwrap();
    }

    fn speak(&self, text: Option<&str>, voice: Option<&str>) {
        let req = SpeakRequest {
            text: text.map(str::to_owned),
            voice: voice.map(|v| VoiceName::parse(v).unwrap()),
        };
        speak(&self.commands, &self.config, req);
    }

    fn state(&self) -> State {
        ask(&self.commands, |reply| Command::Status { reply })
    }

    fn wait_state(&self, want: State) {
        wait_for(&format!("state {want:?}"), WAIT, || {
            (self.state() == want).then_some(())
        });
    }

    fn stop(&self) {
        ask(&self.commands, |reply| Command::Stop { reply });
    }

    fn follow(&self) -> BufReader<UnixStream> {
        let (server, client) = UnixStream::pair().unwrap();
        client.set_read_timeout(Some(WAIT)).unwrap();
        self.commands.send(Command::Follow(server)).unwrap();
        BufReader::new(client)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn set(&self, name: &str, text: &str) {
        std::fs::write(self.path(name), text).unwrap();
    }

    fn notify_log(&self) -> String {
        std::fs::read_to_string(self.path("notify.log")).unwrap()
    }

    fn wait_log(&self, prefix: &str, count: usize) {
        let log = self.path("play.log");
        wait_for(&format!("{count} {prefix:?} lines"), WAIT, || {
            (log_lines(&log, prefix).len() >= count).then_some(())
        });
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

fn next_line(follower: &mut BufReader<UnixStream>) -> std::io::Result<String> {
    let mut line = String::new();
    follower.read_line(&mut line)?;
    Ok(line)
}

fn call(text: &str, voice: &str, speed: f32) -> Call {
    (text.into(), voice.into(), speed)
}

/// Holds synthesis before piece `at`. `reached` fires when a job gets
/// there; dropping `release` lets it continue.
struct Gate {
    reached: Receiver<()>,
    release: Sender<()>,
}

fn gate(at: usize) -> (RecordingEngine, Gate) {
    let (reached_tx, reached) = channel();
    let (release, held) = channel::<()>();
    let (reached_tx, held) = (Mutex::new(reached_tx), Mutex::new(held));
    let before = Arc::new(move |i: usize| {
        if i == at {
            let _ = reached_tx.lock().unwrap().send(());
            let _ = held.lock().unwrap().recv_timeout(WAIT);
        }
    });
    (
        RecordingEngine {
            before: Some(before),
            ..Default::default()
        },
        Gate { reached, release },
    )
}

/// Before piece `at`, waits until `started` fake players have started.
/// The fake arms its TERM trap before logging `start`, so a cut is
/// visible as a `killed` line.
fn players_armed_before(dir: &Path, at: usize, started: usize) -> Arc<dyn Fn(usize) + Send + Sync> {
    let log = dir.join("play.log");
    Arc::new(move |i| {
        if i == at {
            wait_for("players start", WAIT, || {
                (log_lines(&log, "start").len() >= started).then_some(())
            });
        }
    })
}

fn two_pieces() -> (String, String) {
    (
        format!("{}.", "A".repeat(90)),
        format!("{}.", "B".repeat(90)),
    )
}

#[test]
fn speak_binds_configured_voice_and_speed() {
    let h = Harness::new(RecordingEngine::default());
    h.speak(Some("One sentence."), None);
    h.wait_state(State::Idle);
    assert_eq!(h.calls(), [call("One sentence.", "af_heart", 1.0)]);
}

#[test]
fn voice_override_binds_override_and_configured_speed() {
    let h = Harness::new(RecordingEngine::default());
    h.write_config(&[("speed", "1.5".into())]);
    h.speak(Some("Hi, I'm bella."), Some("af_bella"));
    h.wait_state(State::Idle);
    assert_eq!(h.calls(), [call("Hi, I'm bella.", "af_bella", 1.5)]);
}

#[test]
fn cli_voice_travels_the_wire_and_binds_without_writing_config() {
    let h = Harness::new(RecordingEngine::default());
    let argv = ["speak", "--voice", "af_bella", "Hi,", "I'm", "bella."].map(String::from);
    let crate::cli::Cli::Speak { text, voice } = crate::cli::parse(&argv).unwrap() else {
        unreachable!()
    };
    let line = Request::Speak(SpeakRequest {
        text,
        voice: voice.as_deref().and_then(VoiceName::parse),
    })
    .encode();
    let before = std::fs::read(&h.config).unwrap();

    let (server, client) = UnixStream::pair().unwrap();
    let (commands, config) = (h.commands.clone(), h.config.clone());
    std::thread::spawn(move || connection(server, &commands, &config));
    writeln!(&client, "{line}").unwrap();
    let mut reply = String::new();
    BufReader::new(&client).read_line(&mut reply).unwrap();
    assert_eq!(reply, "ok\n");

    h.wait_state(State::Idle);
    assert_eq!(h.calls(), [call("Hi, I'm bella.", "af_bella", 1.0)]);
    assert_eq!(std::fs::read(&h.config).unwrap(), before);
}

#[test]
fn back_to_back_overrides_bind_distinct_voices() {
    let h = Harness::new(RecordingEngine::default());
    h.speak(Some("Hi, I'm bella."), Some("af_bella"));
    h.wait_state(State::Idle);
    h.speak(Some("Hi, I'm george."), Some("bm_george"));
    h.wait_state(State::Idle);
    let voices: Vec<_> = h.calls().into_iter().map(|c| c.1).collect();
    assert_eq!(voices, ["af_bella", "bm_george"]);
}

#[test]
fn next_utterance_binds_new_config() {
    let h = Harness::new(RecordingEngine::default());
    h.speak(Some("First utterance."), None);
    h.wait_state(State::Idle);
    h.write_config(&[("voice", "\"af_bella\"".into()), ("speed", "1.5".into())]);
    h.speak(Some("Second utterance."), None);
    h.wait_state(State::Idle);
    assert_eq!(
        h.calls(),
        [
            call("First utterance.", "af_heart", 1.0),
            call("Second utterance.", "af_bella", 1.5)
        ]
    );
}

#[test]
fn players_start_without_waiting_for_synthesis() {
    let (engine, gate) = gate(0);
    let h = Harness::new(engine);
    h.speak(Some("One sentence."), None);
    gate.reached.recv_timeout(WAIT).unwrap();
    h.wait_log("start", 1);
    assert!(h.calls().is_empty(), "nothing synthesized yet");
    drop(gate.release);
    h.wait_state(State::Idle);
}

#[test]
fn in_flight_utterance_keeps_its_bindings() {
    let (engine, gate) = gate(1);
    let h = Harness::new(engine);
    let (first, second) = two_pieces();
    h.speak(Some(&format!("{first} {second}")), None);
    gate.reached.recv_timeout(WAIT).unwrap();
    h.write_config(&[("voice", "\"af_bella\"".into()), ("speed", "1.5".into())]);
    drop(gate.release);
    h.wait_state(State::Idle);
    assert_eq!(
        h.calls(),
        [
            call(&first, "af_heart", 1.0),
            call(&second, "af_heart", 1.0)
        ]
    );
}

#[test]
fn player_exit_mid_utterance_sets_error_and_notifies() {
    let h = Harness::with(|dir| {
        let log = dir.join("play.log");
        // The player is gone before piece 2.
        let before = Arc::new(move |i: usize| {
            if i == 1 {
                wait_for("player gone", WAIT, || {
                    (log_lines(&log, "gone").len() == 1).then_some(())
                });
            }
        });
        RecordingEngine {
            before: Some(before),
            ..Default::default()
        }
    });
    // Closes stdin before logging, so "gone" means writes now fail.
    let player = h.dir.script(
        "exit-after-one-byte",
        &format!(
            "#!/bin/sh\necho \"start $$\" >> {l}\ndd bs=1 count=1 of=/dev/null 2>/dev/null\nexec 0<&-\necho \"gone $$\" >> {l}\n",
            l = h.path("play.log").display()
        ),
    );
    h.write_config(&[("player", serde_json::to_string(&[player]).unwrap())]);

    let (first, second) = two_pieces();
    h.speak(Some(&format!("{first} {second}")), None);
    h.wait_state(State::Error);
    assert!(h.notify_log().contains("error: player exited"));
}

#[test]
fn first_piece_error_reaps_players_and_notifies() {
    let h = Harness::with(|dir| RecordingEngine {
        before: Some(players_armed_before(dir, 0, 1)),
        fail_at: Some(0),
        ..Default::default()
    });
    h.speak(Some("One sentence."), None);
    h.wait_state(State::Error);
    h.wait_log("killed", 1);
    assert!(h.notify_log().contains("error: boom"));
}

#[test]
fn later_piece_error_reaps_player_and_notifies() {
    let h = Harness::with(|dir| RecordingEngine {
        before: Some(players_armed_before(dir, 1, 1)),
        fail_at: Some(1),
        ..Default::default()
    });
    let (first, second) = two_pieces();
    h.speak(Some(&format!("{first} {second}")), None);
    h.wait_state(State::Error);
    h.wait_log("killed", 1);
    assert!(h.notify_log().contains("error: boom"));
}

#[test]
fn error_persists_until_stop() {
    let h = Harness::new(RecordingEngine {
        fail_at: Some(0),
        ..Default::default()
    });
    h.speak(Some("One sentence."), None);
    h.wait_state(State::Error);
    h.speak(None, None);
    assert_eq!(
        h.state(),
        State::Error,
        "an empty press while in error changes nothing"
    );
    h.stop();
    assert_eq!(h.state(), State::Idle);
}

#[test]
fn empty_press_uses_selection() {
    let h = Harness::new(RecordingEngine::default());
    h.set("capture.txt", "From the selection.");
    h.speak(None, None);
    h.wait_state(State::Idle);
    assert_eq!(h.calls(), [call("From the selection.", "af_heart", 1.0)]);
}

#[test]
fn empty_selection_falls_back_to_clipboard_when_idle() {
    let h = Harness::new(RecordingEngine::default());
    h.set("clipboard.txt", "From the clipboard instead.");
    h.speak(None, None);
    h.wait_state(State::Idle);
    assert_eq!(
        h.calls(),
        [call("From the clipboard instead.", "af_heart", 1.0)]
    );
}

#[test]
fn empty_sources_notify_nothing_to_read_before_the_reply() {
    let h = Harness::new(RecordingEngine::default());
    h.speak(None, None);
    assert_eq!(h.state(), State::Idle);
    assert!(h.calls().is_empty());
    assert!(h.notify_log().contains("nothing to read"));
    assert!(
        log_lines(&h.path("play.log"), "").is_empty(),
        "no player spawned"
    );
}

#[test]
fn empty_press_survives_missing_notify_binary() {
    let h = Harness::new(RecordingEngine::default());
    h.write_config(&[("notify", "[\"/no-such-omatalk-notify\"]".into())]);
    h.speak(None, None);
    assert_eq!(h.state(), State::Idle);
    assert!(h.calls().is_empty());
}

#[test]
fn inline_text_skips_selection() {
    let h = Harness::new(RecordingEngine::default());
    h.set("capture.txt", "Ignored selection.");
    h.speak(Some("Only inline text."), None);
    h.wait_state(State::Idle);
    assert_eq!(h.calls(), [call("Only inline text.", "af_heart", 1.0)]);
}

#[test]
fn stop_press_with_empty_selection_never_reads_the_clipboard() {
    let (engine, gate) = gate(1);
    let h = Harness::new(engine);
    let reads = h.path("clipboard-reads");
    let clipboard = h.dir.script(
        "clipboard",
        &format!("#!/bin/sh\necho read >> {}\n", reads.display()),
    );
    h.write_config(&[(
        "capture_clipboard",
        serde_json::to_string(&[clipboard]).unwrap(),
    )]);
    let (first, second) = two_pieces();
    h.set("capture.txt", &format!("{first} {second}"));
    h.speak(None, None);
    gate.reached.recv_timeout(WAIT).unwrap();
    h.wait_log("start", 1);

    h.set("capture.txt", "");
    h.speak(None, None);
    assert_eq!(h.state(), State::Idle);
    assert!(
        !reads.exists(),
        "a stop press must not wait on the clipboard"
    );
    h.wait_log("killed", 1);
    assert!(!h.notify_log().contains("nothing to read"));

    drop(gate.release);
    h.speak(None, None);
    assert_eq!(
        log_lines(&reads, "read").len(),
        1,
        "an idle empty press does read it"
    );
    assert!(h.notify_log().contains("nothing to read"));
}

#[test]
fn new_selection_cuts_the_playing_utterance_and_speaks_the_new_one() {
    let (engine, gate) = gate(1);
    let h = Harness::new(engine);
    let (first, second) = two_pieces();
    h.set("capture.txt", &format!("{first} {second}"));
    h.speak(None, None);
    gate.reached.recv_timeout(WAIT).unwrap();
    h.wait_log("start", 1);

    h.set("capture.txt", "Completely new selection.");
    h.speak(None, None);
    drop(gate.release);
    h.wait_state(State::Idle);
    h.wait_log("killed", 1);
    h.wait_log("end", 1);
    assert_eq!(
        h.calls(),
        [
            call(&first, "af_heart", 1.0),
            call("Completely new selection.", "af_heart", 1.0)
        ]
    );
}

#[test]
fn followers_see_each_change_once_and_no_idle_between_interrupts() {
    let (engine, gate) = gate(1);
    let h = Harness::new(engine);
    let mut lines = h.follow();
    assert_eq!(next_line(&mut lines).unwrap(), "idle\n");

    let (first, second) = two_pieces();
    h.speak(Some(&format!("{first} {second}")), None);
    gate.reached.recv_timeout(WAIT).unwrap();
    h.speak(Some("Quick replacement."), None);
    assert_eq!(h.state(), State::Speaking);
    drop(gate.release);
    h.wait_state(State::Idle);

    assert_eq!(next_line(&mut lines).unwrap(), "speaking\n");
    assert_eq!(next_line(&mut lines).unwrap(), "idle\n");
    lines
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    assert!(next_line(&mut lines).is_err(), "no further lines");
}

#[test]
fn follower_registered_mid_utterance_sees_speaking_then_idle() {
    let (engine, gate) = gate(1);
    let h = Harness::new(engine);
    let (first, second) = two_pieces();
    h.speak(Some(&format!("{first} {second}")), None);
    gate.reached.recv_timeout(WAIT).unwrap();
    let mut lines = h.follow();
    assert_eq!(next_line(&mut lines).unwrap(), "speaking\n");
    drop(gate.release);
    assert_eq!(next_line(&mut lines).unwrap(), "idle\n");
}

#[test]
fn stale_generation_stream_ended_is_ignored() {
    let (engine, gate) = gate(1);
    let h = Harness::new(engine);
    let (first, second) = two_pieces();
    h.speak(Some(&format!("{first} {second}")), None);
    gate.reached.recv_timeout(WAIT).unwrap();
    h.commands
        .send(Command::StreamEnded {
            generation: Gen(99),
            outcome: Outcome::Failed("stale".into()),
        })
        .unwrap();
    h.commands
        .send(Command::StreamEnded {
            generation: Gen(0),
            outcome: Outcome::Finished,
        })
        .unwrap();
    assert_eq!(h.state(), State::Speaking);
    drop(gate.release);
    h.wait_state(State::Idle);
}

#[test]
fn a_stuck_follower_does_not_stall_status() {
    let h = Harness::new(RecordingEngine::default());
    let (server, _client) = UnixStream::pair().unwrap();
    // A tiny send buffer so a few transitions fill it.
    let size: libc::c_int = 1;
    // SAFETY: valid fd and an int-sized option value.
    unsafe {
        libc::setsockopt(
            server.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_SNDBUF,
            (&raw const size).cast(),
            size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    h.commands.send(Command::Follow(server)).unwrap();
    for i in 0..40 {
        h.speak(Some(&format!("Utterance {i}.")), None);
        h.stop();
    }
    let started = Instant::now();
    assert_eq!(h.state(), State::Idle);
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn bind_refuses_while_a_live_daemon_answers_and_replaces_a_stale_socket() {
    let dir = TempDir::new("bind");
    let paths = Paths {
        socket: dir.path().join("nested/omatalk.sock"),
        config: dir.path().join("config.toml"),
        models: dir.path().to_owned(),
        site: String::new(),
        fake_engine: true,
    };
    let live = bind(&paths).unwrap();
    assert!(bind(&paths).unwrap_err().contains("daemon already running"));
    drop(live);
    // A sibling test's fork can hold the listener fd until its exec.
    wait_for("listener closed", WAIT, || {
        std::os::unix::net::UnixStream::connect(&paths.socket)
            .is_err()
            .then_some(())
    });
    assert!(
        Path::new(&paths.socket).exists(),
        "stale socket left behind"
    );
    bind(&paths).unwrap();
}
