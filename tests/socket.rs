mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;

use common::{
    FAKES, TempDir, fake_commands, fake_env, log_lines, omatalk, read, run, wait_for, write,
};

const WAIT: Duration = Duration::from_secs(20);

/// `omatalk daemon` on the fake engine and shell fakes, in its own temp dir.
struct Daemon {
    child: Child,
    tmp: TempDir,
    env: Vec<(&'static str, PathBuf)>,
}

impl Daemon {
    fn start() -> Daemon {
        Daemon::with_player("player")
    }

    fn with_player(player: &str) -> Daemon {
        let tmp = TempDir::new("socket");
        let dir = tmp.path();
        let config: String = fake_commands(|path| format!("[\"{path}\"]"))
            .into_iter()
            .map(|(key, value)| match key {
                "player" => format!("player = [\"{FAKES}/{player}\"]\n"),
                _ => format!("{key} = {value}\n"),
            })
            .collect();
        write(dir.join("config.toml"), config);
        write(dir.join("ticks.txt"), "1");
        let mut env: Vec<(&str, PathBuf)> = fake_env(dir).into();
        env.extend([
            ("OMATALK_CONFIG", dir.join("config.toml")),
            ("OMATALK_SOCKET", dir.join("d.sock")),
            ("OMATALK_TEST_FAKE_ENGINE", PathBuf::from("1")),
        ]);
        let mut child = Command::new(common::OMATALK)
            .arg("daemon")
            .envs(env.clone())
            .stderr(std::fs::File::create(dir.join("daemon.log")).unwrap())
            .spawn()
            .unwrap();
        let sock = dir.join("d.sock");
        wait_for("daemon socket", Duration::from_secs(60), || {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("daemon died: {status}\n{}", read(dir.join("daemon.log")));
            }
            sock.exists().then_some(())
        });
        Daemon { child, tmp, env }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn connect(&self) -> UnixStream {
        let client = UnixStream::connect(self.path("d.sock")).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        client
    }

    /// One request line; the reply with surrounding whitespace trimmed.
    fn send(&self, line: &str) -> String {
        let mut client = self.connect();
        client.write_all(format!("{line}\n").as_bytes()).unwrap();
        let mut reply = [0u8; 1024];
        let n = client.read(&mut reply).unwrap();
        String::from_utf8_lossy(&reply[..n]).trim().to_owned()
    }

    fn follow(&self) -> BufReader<UnixStream> {
        let mut client = self.connect();
        client.write_all(b"follow\n").unwrap();
        BufReader::new(client)
    }

    fn wait_status(&self, want: &str) {
        wait_for(&format!("status {want:?}"), WAIT, || {
            (self.send("status") == want).then_some(())
        });
    }

    fn wait_log(&self, prefix: &str, count: usize) {
        let log = self.path("play.log");
        wait_for(
            &format!("{count} {prefix:?} lines in play.log"),
            WAIT,
            || (log_lines(&log, prefix).len() >= count).then_some(()),
        );
    }

    fn played(&self, prefix: &str) -> usize {
        log_lines(&self.path("play.log"), prefix).len()
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.path("play.log")).unwrap_or_default()
    }

    fn set_play_ticks(&self, ticks: &str) {
        write(self.path("ticks.txt"), ticks);
    }

    fn set_capture(&self, text: &str) {
        write(self.path("capture.txt"), text);
        write(self.path("clipboard.txt"), "");
    }

    fn clear_logs(&self) {
        write(self.path("play.log"), "");
        write(self.path("notify.log"), "");
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        let _ = self.child.wait();
    }
}

fn speak_line(text: Option<&str>, voice: Option<&str>) -> String {
    let mut payload = serde_json::Map::new();
    if let Some(text) = text {
        payload.insert("text".into(), text.into());
    }
    if let Some(voice) = voice {
        payload.insert("voice".into(), voice.into());
    }
    format!("speak {}", serde_json::Value::Object(payload))
}

fn next_line(reader: &mut BufReader<UnixStream>) -> String {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    line.trim().to_owned()
}

#[test]
fn unknown_command() {
    let daemon = Daemon::start();
    assert_eq!(daemon.send("frobnicate"), "unknown command");
}

#[test]
fn speak_payload_that_is_not_json_is_unknown() {
    let daemon = Daemon::start();
    assert_eq!(daemon.send("speak Only inline text."), "unknown command");
    assert_eq!(daemon.send("status"), "idle");
}

#[test]
fn cli_speak_sends_multiline_and_option_looking_text() {
    let daemon = Daemon::start();
    daemon.set_play_ticks("1");
    let cases: [&[&str]; 2] = [&["--", "--voice", "x"], &["line one\nline two"]];
    for args in cases {
        let result = run(omatalk(["speak"]).args(args).envs(daemon.env.clone()), "");
        assert_eq!(result.code, 0, "{args:?}: {}", result.stderr);
        assert_eq!(result.stdout, "ok\n", "{args:?}");
        daemon.wait_status("idle");
    }
}

/// PCM bytes the capturing player received for one spoken Utterance.
fn spoken_bytes(daemon: &Daemon, text: &str) -> usize {
    write(daemon.path("pcm.raw"), "");
    assert_eq!(daemon.send(&speak_line(Some(text), None)), "ok");
    daemon.wait_status("idle");
    std::fs::read(daemon.path("pcm.raw")).unwrap().len()
}

#[test]
fn config_wake_lead_reaches_the_player_on_a_suspended_bluetooth_sink() {
    let daemon = Daemon::with_player("player-capture");
    let config = read(daemon.path("config.toml"));
    let probe = format!("sink_probe = [\"{FAKES}/pactl-suspended-bluetooth\"]\n");
    let set_lead = |ms: u32| {
        write(
            daemon.path("config.toml"),
            config.replace(
                "sink_probe = [\"false\"]\n",
                &format!("{probe}wake_lead_ms = {ms}\n"),
            ),
        )
    };

    set_lead(0);
    let speech = spoken_bytes(&daemon, "One sentence.");
    assert!(speech > 0, "the fake engine produced no PCM");
    set_lead(100);
    assert_eq!(
        spoken_bytes(&daemon, "One sentence."),
        speech + 100 * 24 * 2,
        "100 ms of lead is 2400 frames of s16le ahead of the same speech"
    );
}

#[test]
fn status_idle() {
    let daemon = Daemon::start();
    assert_eq!(daemon.send("status"), "idle");
}

#[test]
fn cli_rejects_follow() {
    let daemon = Daemon::start();
    let result = run(omatalk(["status", "--follow"]).envs(daemon.env.clone()), "");
    assert_eq!(result.code, 2);
    assert!(
        result.stderr.starts_with("usage: omatalk"),
        "{}",
        result.stderr
    );
}

#[test]
fn follow_streams_state() {
    let daemon = Daemon::start();
    let mut follower = daemon.follow();
    assert_eq!(next_line(&mut follower), "idle");
    daemon.set_play_ticks("20");
    daemon.set_capture("One sentence to watch. And a second one.");
    assert_eq!(daemon.send("speak"), "ok");
    assert_eq!(next_line(&mut follower), "speaking");
    assert_eq!(next_line(&mut follower), "idle");
    drop(follower);
    // The follow connection must not have blocked the accept loop.
    assert_eq!(daemon.send("status"), "idle");
}

#[test]
fn follow_catches_change_before_initial_reply_is_read() {
    let daemon = Daemon::start();
    let mut follower = daemon.follow();
    daemon.set_play_ticks("20");
    daemon.set_capture("Immediate follow transition.");
    assert_eq!(daemon.send("speak"), "ok");

    assert_eq!(next_line(&mut follower), "idle");
    assert_eq!(next_line(&mut follower), "speaking");
    assert_eq!(next_line(&mut follower), "idle");
}

#[test]
fn follow_supports_multiple_bar_instances() {
    let daemon = Daemon::start();
    let mut followers = [daemon.follow(), daemon.follow()];
    let next = |followers: &mut [BufReader<UnixStream>; 2]| followers.each_mut().map(next_line);

    assert_eq!(next(&mut followers), ["idle", "idle"]);
    daemon.set_play_ticks("20");
    daemon.set_capture("Two bar instances.");
    assert_eq!(daemon.send("speak"), "ok");
    assert_eq!(next(&mut followers), ["speaking", "speaking"]);
    assert_eq!(next(&mut followers), ["idle", "idle"]);
}

#[test]
fn speak_captured_text_uses_one_player_for_the_utterance() {
    let daemon = Daemon::start();
    daemon.clear_logs();
    let first = format!("{}.", "A".repeat(90));
    let second = format!("{}.", "B".repeat(90));
    daemon.set_capture(&format!("{first} {second}"));
    assert_eq!(daemon.send("speak"), "ok");
    daemon.wait_status("speaking");
    daemon.wait_status("idle");
    assert_eq!(daemon.played("start"), 1, "{}", daemon.log());
    assert_eq!(daemon.played("end"), 1, "{}", daemon.log());
}

#[test]
fn stop_cuts_playback() {
    let daemon = Daemon::start();
    daemon.clear_logs();
    daemon.set_play_ticks("30");
    daemon.set_capture("A sentence that plays for a while. And another.");
    assert_eq!(daemon.send("speak"), "ok");
    daemon.wait_status("speaking");
    daemon.wait_log("start", 1);
    assert_eq!(daemon.send("stop"), "ok");
    assert_eq!(daemon.send("status"), "idle");
    // The cut is asynchronous: the reply does not wait on reaping.
    daemon.wait_log("killed", 1);
}

#[test]
fn cli_waits_out_the_daemons_slowest_speak_reply() {
    let daemon = Daemon::start();
    write(
        daemon.path("config.toml"),
        "capture_primary = [\"sleep\", \"10\"]\n\
         capture_clipboard = [\"sleep\", \"10\"]\n\
         notify = [\"sh\", \"-c\", \"sleep 10\"]\n",
    );

    let result = run(omatalk(["speak"]).envs(daemon.env.clone()), "");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "ok\n");
}
