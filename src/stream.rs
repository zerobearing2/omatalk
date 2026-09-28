//! A Stream is one Utterance in flight: synthesis on the shared synth thread
//! feeding a playback thread that owns this Utterance's pw-cat processes.
//!
//! Ownership (no locks, no shared mutable state):
//! - the actor owns the `Stream` value; dropping it fires the `StopToken`.
//! - the synth thread owns the engine; it only sends PCM down a bounded
//!   channel and never talks to the actor.
//! - the playback thread owns the player process and is the single
//!   reporter of the Utterance's outcome (`on_end`).

use std::io::Write;
use std::os::fd::AsRawFd;
use std::panic::{self, AssertUnwindSafe};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender};
use std::time::{Duration, Instant};

use crate::exec::Argv;
use crate::sink::WakeLead;
use crate::speech::{Engine, SAMPLE_RATE, SpeechError, StopToken, Utterance};

/// How often the playback thread checks its token while waiting for PCM or
/// for pipe space. Bounds Interrupt latency; only ticks while speaking.
pub const TICK: Duration = Duration::from_millis(10);
/// Synth may run this many batches ahead of playback.
pub const PCM_QUEUE: usize = 4;
/// After SIGTERM + stdin close, how long a player gets before SIGKILL.
pub const REAP_GRACE: Duration = Duration::from_secs(1);
/// The panic hook in `daemon::serve` lets this thread's panics unwind into
/// the per-job `catch_unwind`; a panic anywhere else aborts the process.
pub const SYNTH_THREAD: &str = "omatalk-synth";

/// Monotonic per Daemon. The actor ignores any report whose Gen is not the
/// current Stream's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gen(pub u64);

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// All audio was written and the speech player exited after EOF.
    Finished,
    /// Text for the notify after `error: `: `player exited` or the engine's message.
    Failed(String),
    /// The token fired first. Never notified; the actor has moved on.
    Stopped,
}

/// The per-Utterance processes a Stream needs, from the config snapshot
/// taken at Utterance start.
#[derive(Clone, Debug)]
pub struct StreamIo {
    pub player: Argv,
    pub wake: WakeLead,
}

/// Handle to the single synth thread. Cloneable sender; the thread exits when
/// every handle is gone.
#[derive(Clone)]
pub struct Synth {
    jobs: Sender<Job>,
}

struct Job {
    utterance: Utterance,
    stop: StopToken,
    pcm: SyncSender<Pcm>,
}

enum Pcm {
    Samples(Vec<f32>),
    End,
    Failed(SpeechError),
}

impl Synth {
    /// Moves the engine onto its own thread. Jobs run one at a time in order;
    /// a job whose token already fired is skipped without touching the engine.
    pub fn spawn(engine: Box<dyn Engine>) -> Synth {
        let (jobs, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name(SYNTH_THREAD.into())
            .spawn(move || synth_loop(engine, rx))
            .expect("spawn synth thread");
        Synth { jobs }
    }
}

fn synth_loop(mut engine: Box<dyn Engine>, jobs: Receiver<Job>) {
    for Job {
        utterance,
        stop,
        pcm,
    } in jobs
    {
        if stop.is_fired() {
            continue;
        }
        // The engine keeps no state across jobs that a panic could corrupt,
        // so odd input costs one Utterance, not the Daemon.
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut audio = engine.speak(&utterance, &stop);
            while !stop.is_fired() {
                let Some(samples) = audio.next() else { break };
                // A closed channel means playback is gone (player died or stop).
                if pcm.send(Pcm::Samples(samples?)).is_err() {
                    break;
                }
            }
            Ok(())
        }));
        let _ = pcm.send(match result {
            Ok(Ok(())) => Pcm::End,
            Ok(Err(e)) => Pcm::Failed(e),
            Err(panic) => Pcm::Failed(SpeechError(format!(
                "internal error: {}",
                panic_message(&*panic)
            ))),
        });
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("panic")
}

/// One Utterance in flight. Not Clone: exactly one owner can stop it.
pub struct Stream {
    generation: Gen,
    stop: StopToken,
}

impl Stream {
    /// Queues synthesis and starts the playback thread, which spawns the
    /// speech player and probes the sink in parallel with the first ORT run.
    /// `on_end` is called exactly once, from the playback thread.
    pub fn start(
        utterance: Utterance,
        io: StreamIo,
        generation: Gen,
        synth: &Synth,
        on_end: impl FnOnce(Gen, Outcome) + Send + 'static,
    ) -> Stream {
        let stop = StopToken::default();
        let (pcm_tx, pcm_rx) = std::sync::mpsc::sync_channel(PCM_QUEUE);
        let playback_stop = stop.clone();
        std::thread::Builder::new()
            .name("omatalk-play".into())
            .spawn(move || {
                let outcome = playback(&io, pcm_rx, &playback_stop);
                // Stopped wins: an interrupted Utterance is never an error.
                on_end(
                    generation,
                    if playback_stop.is_fired() {
                        Outcome::Stopped
                    } else {
                        outcome
                    },
                );
            })
            .expect("spawn playback thread");
        let job = Job {
            utterance,
            stop: stop.clone(),
            pcm: pcm_tx,
        };
        // The synth thread only exits with the process, so this cannot fail.
        let _ = synth.jobs.send(job);
        Stream { generation, stop }
    }

    pub fn generation(&self) -> Gen {
        self.generation
    }
}

impl Drop for Stream {
    /// Interrupt: aborts the in-flight ORT run and makes the playback thread
    /// cut the player (TERM, close stdin, grace, KILL) within one TICK.
    /// Never blocks the caller.
    fn drop(&mut self) {
        self.stop.fire();
    }
}

/// Playback thread body.
///
/// Invariant: on every exit path `pcm` is dropped BEFORE any player is cut.
/// A synth thread blocked on a full queue then unblocks at once instead of
/// waiting out REAP_GRACE, so the next Utterance's first ORT run is not delayed.
fn playback(io: &StreamIo, pcm: Receiver<Pcm>, stop: &StopToken) -> Outcome {
    let Ok(mut speech) = Player::spawn(&io.player) else {
        return PlayerExited.into();
    };
    // Synthesis runs meanwhile; the probe is bounded well under first PCM.
    let lead_frames = io.wake.measure().as_millis() as usize * SAMPLE_RATE as usize / 1000;
    let fed = speech
        .write(&s16le(&vec![0.0; lead_frames]), stop)
        .map_err(Outcome::from)
        .and_then(|()| feed(&mut speech, &pcm, stop));
    drop(pcm);
    match fed {
        Ok(()) => speech
            .finish(stop)
            .map_or_else(Outcome::from, |()| Outcome::Finished),
        Err(outcome) => {
            speech.cut();
            outcome
        }
    }
}

/// Writes PCM to `speech` as it arrives until synthesis ends (`Ok`), or the
/// player exits, synthesis fails, or the token fires.
fn feed(speech: &mut Player, pcm: &Receiver<Pcm>, stop: &StopToken) -> Result<(), Outcome> {
    loop {
        if stop.is_fired() {
            return Err(Outcome::Stopped);
        }
        match pcm.recv_timeout(TICK) {
            Ok(Pcm::Samples(samples)) => speech.write(&s16le(&samples), stop)?,
            // Disconnected: the job was skipped (token fired) or the synth
            // thread is gone; either way no more PCM is coming.
            Ok(Pcm::End) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Ok(Pcm::Failed(e)) => return Err(Outcome::Failed(e.0)),
            Err(RecvTimeoutError::Timeout) => {
                if !running(&mut speech.child) {
                    return Err(PlayerExited.into());
                }
            }
        }
    }
}

/// f32 to s16le, clipped, truncated toward zero.
fn s16le(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect()
}

/// One pw-cat (or compatible) process fed raw s16le mono at 24 kHz.
struct Player {
    child: Child,
    /// Non-blocking, so a full pipe never hides a stop.
    stdin: ChildStdin,
}

#[derive(Debug)]
struct PlayerExited;

impl From<PlayerExited> for Outcome {
    fn from(_: PlayerExited) -> Outcome {
        Outcome::Failed("player exited".into())
    }
}

impl Player {
    /// `argv + ["--rate", "24000", "--channels", "1", "-"]`, stdin piped and
    /// set O_NONBLOCK, stdout/stderr null.
    fn spawn(argv: &Argv) -> std::io::Result<Player> {
        let mut child = argv
            .command()
            .args(["--rate", &SAMPLE_RATE.to_string(), "--channels", "1", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let fd = stdin.as_raw_fd();
        // SAFETY: fd is a valid pipe fd owned by `stdin` for this whole call.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        Ok(Player { child, stdin })
    }

    /// Writes all of `bytes`, polling for pipe space in TICK slices so a stop
    /// is seen within one TICK even when the player is not reading. Returns
    /// Ok early (writing nothing more) once the token fires.
    fn write(&mut self, mut bytes: &[u8], stop: &StopToken) -> Result<(), PlayerExited> {
        let stdin = &mut self.stdin;
        while !bytes.is_empty() {
            if stop.is_fired() {
                return Ok(());
            }
            match stdin.write(bytes) {
                Ok(0) => return Err(PlayerExited),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let mut fd = libc::pollfd {
                        fd: stdin.as_raw_fd(),
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                    // SAFETY: one valid pollfd; a failed poll just loops.
                    unsafe { libc::poll(&mut fd, 1, TICK.as_millis() as libc::c_int) };
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return Err(PlayerExited),
            }
        }
        Ok(())
    }

    /// Normal end: close stdin (EOF), wait for exit, polling the token each
    /// TICK (a stop during drain is cut). Non-zero exit is `PlayerExited`.
    fn finish(self, stop: &StopToken) -> Result<(), PlayerExited> {
        let Player { mut child, stdin } = self;
        drop(stdin);
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(_)) | Err(_) => return Err(PlayerExited),
                Ok(None) if stop.is_fired() => {
                    terminate(&mut child);
                    reap(child);
                    return Ok(());
                }
                Ok(None) => std::thread::sleep(TICK),
            }
        }
    }

    /// Interrupt: SIGTERM first so pw-cat does not drain the pipe, then close
    /// stdin (pw-cat ignores TERM while stdin is open), then reap.
    fn cut(self) {
        let Player { mut child, stdin } = self;
        terminate(&mut child);
        drop(stdin);
        reap(child);
    }
}

fn running(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None))
}

/// SIGTERM, only while unreaped so the pid cannot have been reused.
fn terminate(child: &mut Child) {
    if running(child) {
        // SAFETY: kill has no memory preconditions; the pid is our unreaped child.
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
    }
}

/// Waits up to REAP_GRACE for `child` to exit, then SIGKILLs it.
fn reap(mut child: Child) {
    let deadline = Instant::now() + REAP_GRACE;
    while Instant::now() < deadline {
        if !running(&mut child) {
            return;
        }
        std::thread::sleep(TICK);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Speed;
    use crate::speech::{Audio, Text};
    use crate::testutil::{TempDir, log_lines, wait_for};
    use crate::voices::VoiceName;
    use std::fs;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// Yields fixed batches; can block before a batch until the test releases it.
    struct Scripted {
        batches: Vec<Vec<f32>>,
        /// Called with the batch index before it is yielded.
        before: Arc<dyn Fn(usize) + Send + Sync>,
    }

    impl Engine for Scripted {
        fn speak<'a>(&'a mut self, _: &'a Utterance, _: &'a StopToken) -> Audio<'a> {
            Box::new(self.batches.iter().enumerate().map(|(i, batch)| {
                (self.before)(i);
                Ok(batch.clone())
            }))
        }
    }

    struct Panics;

    impl Engine for Panics {
        fn speak<'a>(&'a mut self, _: &'a Utterance, _: &'a StopToken) -> Audio<'a> {
            panic!("odd input");
        }
    }

    fn utterance() -> Utterance {
        Utterance {
            text: Text::new("Hello.").unwrap(),
            voice: VoiceName::parse("af_heart").unwrap(),
            speed: Speed::parse("1").unwrap(),
        }
    }

    fn argv(path: &Path) -> Argv {
        Argv::new(vec![path.display().to_string()]).unwrap()
    }

    /// A probe that fails, so no lead: the pre-lead behavior.
    fn io(player: &Path) -> StreamIo {
        StreamIo {
            player: argv(player),
            wake: WakeLead {
                probe: Argv::new(vec!["false".into()]).unwrap(),
                lead: Duration::from_millis(600),
            },
        }
    }

    /// A pactl that reports `sink` as the default sink, in `state`.
    fn probe(dir: &TempDir, sink: &str, state: &str) -> Argv {
        argv(&dir.script(
            "pactl",
            &format!(
                "#!/bin/sh\ncase \"$*\" in\n  get-default-sink) echo {sink} ;;\n  'list sinks short') printf '98\\t{sink}\\tPipeWire\\ts16le 2ch 48000Hz\\t{state}\\n' ;;\n  *) exit 1 ;;\nesac\n"
            ),
        ))
    }

    /// Starts one Stream; the receiver yields its single outcome.
    fn start(
        engine: impl Engine + 'static,
        player: &Path,
    ) -> (Stream, std::sync::mpsc::Receiver<Outcome>) {
        start_io(engine, io(player))
    }

    fn start_io(
        engine: impl Engine + 'static,
        io: StreamIo,
    ) -> (Stream, std::sync::mpsc::Receiver<Outcome>) {
        let synth = Synth::spawn(Box::new(engine));
        let (tx, rx) = std::sync::mpsc::channel();
        let stream = Stream::start(utterance(), io, Gen(1), &synth, move |_, outcome| {
            tx.send(outcome).unwrap();
        });
        (stream, rx)
    }

    fn batches(batches: Vec<Vec<f32>>) -> Scripted {
        Scripted {
            batches,
            before: Arc::new(|_| {}),
        }
    }

    /// Blocks before batch `at` until the returned sender is dropped.
    fn gated(batches: Vec<Vec<f32>>, at: usize) -> (Scripted, std::sync::mpsc::Sender<()>) {
        let (release, held) = std::sync::mpsc::channel::<()>();
        let held = Mutex::new(held);
        let before = Arc::new(move |i: usize| {
            if i == at {
                let _ = held.lock().unwrap().recv_timeout(Duration::from_secs(10));
            }
        });
        (Scripted { batches, before }, release)
    }

    /// Yields `first`, then holds.
    fn then_hold(first: Vec<f32>) -> (Scripted, std::sync::mpsc::Sender<()>) {
        gated(vec![first, vec![0.0; 24]], 1)
    }

    fn outcome(rx: &std::sync::mpsc::Receiver<Outcome>) -> Outcome {
        rx.recv_timeout(Duration::from_secs(10))
            .expect("stream ended")
    }

    /// A player that records its argv and everything it reads, per pid.
    fn echo_player(dir: &TempDir) -> std::path::PathBuf {
        let d = dir.path().display();
        dir.script(
            "echo-player",
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {d}/args.log\ncat >> {d}/captured.$$.bin\n"
            ),
        )
    }

    fn captured(dir: &TempDir) -> Vec<Vec<u8>> {
        fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("captured.")
            })
            .map(|p| fs::read(p).unwrap())
            .collect()
    }

    #[test]
    fn feeds_s16le_on_stdin_with_rate_and_channel_args() {
        let dir = TempDir::new("feed");
        let first = vec![0.5, -0.5, 0.25, -0.25];
        let second = vec![1.5, -1.5];
        let (_stream, rx) = start(
            batches(vec![first.clone(), second.clone()]),
            &echo_player(&dir),
        );
        assert_eq!(outcome(&rx), Outcome::Finished);

        let want = [s16le(&first), s16le(&second)].concat();
        assert_eq!(
            want,
            [
                0xff, 0x3f, 0x01, 0xc0, 0xff, 0x1f, 0x01, 0xe0, 0xff, 0x7f, 0x01, 0x80
            ]
        );
        assert_eq!(
            captured(&dir),
            [want],
            "one player got every batch in order"
        );
        let args = fs::read_to_string(dir.path().join("args.log")).unwrap();
        assert_eq!(args, "--rate 24000 --channels 1 -\n");
    }

    #[test]
    fn lead_of_silence_precedes_speech_only_on_a_suspended_bluetooth_or_hdmi_sink() {
        const BT: &str = "bluez_output.F4_2B_7D_4B_D0_63.1";
        const HDMI: &str = "alsa_output.pci-0000_c1_00.1.hdmi-stereo-extra2";
        const ANALOG: &str = "alsa_output.pci-0000_c1_00.6.analog-stereo";
        let samples = vec![0.5, -0.5];
        for (sink, state, lead_ms, want_lead_bytes) in [
            (BT, "SUSPENDED", 600, 28_800),
            (HDMI, "SUSPENDED", 600, 28_800),
            (BT, "SUSPENDED", 250, 12_000),
            (BT, "RUNNING", 600, 0),
            (BT, "IDLE", 600, 0),
            (HDMI, "RUNNING", 600, 0),
            (ANALOG, "SUSPENDED", 600, 0),
            (BT, "SUSPENDED", 0, 0),
        ] {
            let dir = TempDir::new("lead");
            let io = StreamIo {
                wake: WakeLead {
                    probe: probe(&dir, sink, state),
                    lead: Duration::from_millis(lead_ms),
                },
                ..io(&echo_player(&dir))
            };
            let (_stream, rx) = start_io(batches(vec![samples.clone()]), io);
            assert_eq!(outcome(&rx), Outcome::Finished);
            let want = [vec![0; want_lead_bytes], s16le(&samples)].concat();
            assert_eq!(
                captured(&dir),
                [want],
                "{sink} {state} {lead_ms} ms: want {want_lead_bytes} bytes of lead"
            );
        }
    }

    #[test]
    fn failing_missing_or_hung_probe_is_no_lead_and_no_delay() {
        let dir = TempDir::new("bad-probe");
        let hung = dir.script("hung", "#!/bin/sh\nexec sleep 10\n");
        let samples = vec![0.5, -0.5];
        for probe in [
            argv(&dir.script("fails", "#!/bin/sh\nexit 1\n")),
            argv(&dir.path().join("no-such-pactl")),
            argv(&hung),
        ] {
            let run = TempDir::new("bad-probe-run");
            let started = Instant::now();
            let mut io = io(&echo_player(&run));
            io.wake.probe = probe.clone();
            let (_stream, rx) = start_io(batches(vec![samples.clone()]), io);
            wait_for("speech bytes", Duration::from_secs(5), || {
                captured(&run).contains(&s16le(&samples)).then_some(())
            });
            let took = started.elapsed();
            assert!(
                took < crate::sink::PROBE_TIMEOUT + Duration::from_millis(300),
                "{probe:?} delayed first PCM by {took:?}"
            );
            assert_eq!(outcome(&rx), Outcome::Finished);
        }
    }

    #[test]
    fn stop_during_the_lead_sends_term_within_a_tick() {
        let dir = TempDir::new("stop-lead");
        let log = dir.path().join("log");
        let player = dir.script(
            "not-reading",
            &format!(
                "#!/bin/sh\ntrap 'echo killed >> {l}; kill $!; exit 0' TERM\necho start >> {l}\nsleep 100 &\nwait\n",
                l = log.display()
            ),
        );
        // Two seconds of lead is more than a pipe holds, so playback is
        // parked in the lead write when stopped.
        let io = StreamIo {
            wake: WakeLead {
                probe: probe(&dir, "bluez_output.X.1", "SUSPENDED"),
                lead: Duration::from_secs(2),
            },
            ..io(&player)
        };
        let (engine, _release) = gated(vec![vec![0.1; 24]], 0);
        let (stream, rx) = start_io(engine, io);
        wait_for("players started", Duration::from_secs(5), || {
            (!log_lines(&log, "start").is_empty()).then_some(())
        });
        std::thread::sleep(Duration::from_millis(200));

        let stopped_at = Instant::now();
        drop(stream);
        wait_for("killed", Duration::from_secs(1), || {
            (!log_lines(&log, "killed").is_empty()).then_some(())
        });
        let latency = stopped_at.elapsed();
        assert!(latency < Duration::from_millis(50), "stop took {latency:?}");
        assert_eq!(outcome(&rx), Outcome::Stopped);
    }

    #[test]
    fn synth_runs_ahead_of_a_player_that_is_not_reading() {
        let dir = TempDir::new("slow");
        let d = dir.path().display();
        let player = dir.script(
            "slow-player",
            &format!("#!/bin/sh\nsleep 1\ncat >> {d}/captured.$$.bin\n"),
        );
        let yielded = Arc::new(Mutex::new(Vec::new()));
        let seen = yielded.clone();
        let started = Instant::now();
        let engine = Scripted {
            batches: vec![vec![0.1; 200_000]; 3],
            before: Arc::new(move |_| seen.lock().unwrap().push(started.elapsed())),
        };
        let (_stream, rx) = start(engine, &player);
        wait_for("three batches synthesized", Duration::from_secs(5), || {
            (yielded.lock().unwrap().len() == 3).then_some(())
        });
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "synth must not wait on the pipe"
        );
        assert_eq!(outcome(&rx), Outcome::Finished);
        assert!(captured(&dir).contains(&s16le(&vec![0.1; 600_000])));
    }

    #[test]
    fn stop_sends_term_within_a_tick_without_draining() {
        let dir = TempDir::new("stop-latency");
        let log = dir.path().join("log");
        let player = dir.script(
            "not-reading",
            &format!(
                "#!/bin/sh\ntrap 'echo killed >> {l}; kill $!; exit 0' TERM\necho start >> {l}\nsleep 100 &\nwait\necho drained >> {l}\n",
                l = log.display()
            ),
        );
        // More than a pipe holds, so playback is parked in poll when stopped.
        let (engine, _release) = then_hold(vec![0.1; 100_000]);
        let (stream, rx) = start(engine, &player);
        wait_for("player started", Duration::from_secs(5), || {
            (log_lines(&log, "start").len() == 1).then_some(())
        });
        std::thread::sleep(Duration::from_millis(50));

        let stopped_at = Instant::now();
        drop(stream);
        wait_for("killed", Duration::from_secs(1), || {
            (!log_lines(&log, "killed").is_empty()).then_some(())
        });
        let latency = stopped_at.elapsed();
        assert!(latency < Duration::from_millis(50), "stop took {latency:?}");
        assert_eq!(outcome(&rx), Outcome::Stopped);
        assert!(log_lines(&log, "drained").is_empty());
    }

    #[test]
    fn stop_closes_stdin_after_term_so_a_term_ignoring_player_exits() {
        let dir = TempDir::new("ignore-term");
        let log = dir.path().join("log");
        let player = dir.script(
            "ignore-term",
            &format!(
                "#!/bin/sh\ntrap '' TERM\necho ready >> {l}\ncat > /dev/null\necho eof >> {l}\n",
                l = log.display()
            ),
        );
        let (_release, held) = std::sync::mpsc::channel::<()>();
        let held = Mutex::new(held);
        let ready = log.clone();
        let (reached_tx, reached) = std::sync::mpsc::channel();
        let reached_tx = Mutex::new(reached_tx);
        let engine = Scripted {
            batches: vec![vec![0.1; 24], vec![0.1; 24]],
            // TERM before the trap is installed would just kill the shell.
            before: Arc::new(move |i| match i {
                0 => wait_for("trap set", Duration::from_secs(5), || {
                    (log_lines(&ready, "ready").len() == 1).then_some(())
                }),
                _ => {
                    let _ = reached_tx.lock().unwrap().send(());
                    let _ = held.lock().unwrap().recv_timeout(Duration::from_secs(10));
                }
            }),
        };
        let (stream, rx) = start(engine, &player);
        reached
            .recv_timeout(Duration::from_secs(5))
            .expect("first batch sent");

        let stopped_at = Instant::now();
        drop(stream);
        wait_for("speech eof", Duration::from_millis(500), || {
            (log_lines(&log, "eof").len() == 1).then_some(())
        });
        assert!(stopped_at.elapsed() < Duration::from_millis(500));
        assert_eq!(outcome(&rx), Outcome::Stopped);
    }

    #[test]
    fn player_that_exits_nonzero_is_player_exited() {
        let dir = TempDir::new("exit-now");
        let player = dir.script("exit-now", "#!/bin/sh\nexit 1\n");
        let (_stream, rx) = start(batches(vec![vec![0.1; 24]]), &player);
        assert_eq!(outcome(&rx), Outcome::Failed("player exited".into()));
    }

    #[test]
    fn missing_player_is_player_exited() {
        let dir = TempDir::new("missing");
        let (_stream, rx) = start(batches(vec![vec![0.1]]), &dir.path().join("no-such-player"));
        assert_eq!(outcome(&rx), Outcome::Failed("player exited".into()));
    }

    #[test]
    fn stopped_before_any_pcm_is_not_an_error() {
        let dir = TempDir::new("early-stop");
        let (engine, release) = gated(vec![vec![0.1; 24]], 0);
        let (stream, rx) = start(engine, &echo_player(&dir));
        drop(stream);
        drop(release);
        assert_eq!(outcome(&rx), Outcome::Stopped);
    }

    #[test]
    fn synthesis_stops_pulling_once_playback_is_gone() {
        let dir = TempDir::new("gone");
        let pulled = Arc::new(Mutex::new(0));
        let seen = pulled.clone();
        let synth = Synth::spawn(Box::new(Scripted {
            batches: vec![vec![0.1; 24]; 100],
            before: Arc::new(move |_| *seen.lock().unwrap() += 1),
        }));
        let run = |player: &Path| {
            let (tx, rx) = std::sync::mpsc::channel();
            let stream = Stream::start(utterance(), io(player), Gen(1), &synth, move |_, o| {
                tx.send(o).unwrap()
            });
            (stream, rx)
        };
        // Kept alive, so only the closed PCM channel can end its synthesis.
        let (_gone, rx) = run(&dir.path().join("no-such-player"));
        assert_eq!(outcome(&rx), Outcome::Failed("player exited".into()));
        // Jobs run in order: once this one finishes, the first has ended.
        let (_next, rx) = run(&echo_player(&dir));
        assert_eq!(outcome(&rx), Outcome::Finished);
        let first = *pulled.lock().unwrap() - 100;
        assert!(first <= PCM_QUEUE + 1, "pulled {first} batches");
    }

    #[test]
    fn engine_panic_becomes_internal_error_and_synth_keeps_serving() {
        let dir = TempDir::new("panic");
        let synth = Synth::spawn(Box::new(Panics));
        let io = io(&echo_player(&dir));
        for generation in 1..=2 {
            let (tx, rx) = std::sync::mpsc::channel();
            let _stream = Stream::start(
                utterance(),
                io.clone(),
                Gen(generation),
                &synth,
                move |_, o| tx.send(o).unwrap(),
            );
            assert_eq!(
                outcome(&rx),
                Outcome::Failed("internal error: odd input".into())
            );
        }
    }
}
