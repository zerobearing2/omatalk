//! Running the user's configured helper programs: capture (wl-paste) and
//! notify (notify-send), plus `stdout` for the sink probe. The player is
//! `stream.rs`'s, because its lifetime is a Stream's. Nothing here is ever
//! called on the actor thread.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::speech::Text;

/// A non-empty argv from config.toml. `argv[0]` is looked up on PATH.
#[derive(Clone, Debug, PartialEq)]
pub struct Argv {
    program: String,
    args: Vec<String>,
}

impl Argv {
    pub fn new(words: Vec<String>) -> Option<Argv> {
        let mut it = words.into_iter();
        let program = it.next()?;
        Some(Argv {
            program,
            args: it.collect(),
        })
    }

    pub fn command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd
    }

    pub fn words(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.program.as_str()).chain(self.args.iter().map(String::as_str))
    }
}

pub const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);
pub const NOTIFY_TIMEOUT: Duration = Duration::from_secs(2);

/// Stdout of a capture command as a Source. `None` when blank, or on a
/// missing binary, non-zero exit, non-UTF-8 output, or timeout (then the
/// child is killed and reaped): an unreadable Source is an empty Source.
pub fn capture(argv: &Argv) -> Option<Text> {
    stdout(argv.command(), Instant::now() + CAPTURE_TIMEOUT).and_then(|s| Text::new(&s))
}

/// Stdout of `cmd` when it exits successfully by `deadline` with UTF-8
/// output. `None` on a missing binary, non-zero exit, non-UTF-8 output, or
/// timeout (then the child is killed and reaped).
pub fn stdout(mut cmd: Command, deadline: Instant) -> Option<String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Drained on its own thread: a large selection would otherwise fill the
    // pipe and the child would never exit.
    let mut out = child.stdout.take().expect("stdout is piped");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = tx.send(out.read_to_end(&mut bytes).map(|_| bytes));
    });
    let bytes = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    match (wait_until(&mut child, deadline), bytes) {
        (true, Ok(Ok(bytes))) => String::from_utf8(bytes).ok(),
        _ => None,
    }
}

/// Runs `argv + [msg]` and waits for it (bounded). Synchronous on purpose:
/// callers flip visible state only after the notify has happened. A missing
/// notify binary is swallowed.
pub fn notify(argv: &Argv, msg: &str) {
    let spawned = argv
        .command()
        .arg(msg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Ok(mut child) = spawned {
        wait_until(&mut child, Instant::now() + NOTIFY_TIMEOUT);
    }
}

/// True when the child exited successfully by `deadline`. Otherwise it is
/// killed. Either way it is reaped.
fn wait_until(child: &mut Child, deadline: Instant) -> bool {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(1)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Argv {
        Argv::new(vec!["sh".into(), "-c".into(), script.into()]).unwrap()
    }

    #[test]
    fn capture_strips_stdout() {
        assert_eq!(
            capture(&sh("printf '  hello there \\n\\n'")),
            Text::new("hello there")
        );
        assert_eq!(capture(&sh("printf ' \\n'")), None);
    }

    #[test]
    fn capture_is_empty_on_missing_binary_failure_or_bad_utf8() {
        assert_eq!(
            capture(&Argv::new(vec!["/no-such-omatalk-capture".into()]).unwrap()),
            None
        );
        assert_eq!(capture(&sh("echo text; exit 3")), None);
        assert_eq!(capture(&sh("printf '\\377\\376'")), None);
    }

    #[test]
    fn capture_drains_output_larger_than_a_pipe() {
        assert_eq!(
            capture(&sh("head -c 300000 /dev/zero | tr '\\0' x"))
                .unwrap()
                .as_str()
                .len(),
            300_000
        );
    }

    #[test]
    fn capture_times_out_and_reaps() {
        let start = Instant::now();
        assert_eq!(capture(&sh("echo early; exec sleep 10")), None);
        let took = start.elapsed();
        assert!(
            took >= CAPTURE_TIMEOUT && took < CAPTURE_TIMEOUT + Duration::from_secs(1),
            "{took:?}"
        );
    }

    #[test]
    fn notify_swallows_missing_binary() {
        notify(
            &Argv::new(vec!["/no-such-omatalk-notify".into()]).unwrap(),
            "hello",
        );
    }

    #[test]
    fn notify_appends_the_message_and_waits() {
        let dir = crate::testutil::TempDir::new("notify");
        let log = dir.path().join("log");
        let argv = sh(&format!("printf '%s' \"$1\" > {}", log.display()));
        let argv = Argv::new(
            argv.words()
                .map(str::to_owned)
                .chain(["notify".into()])
                .collect(),
        )
        .unwrap();
        notify(&argv, "nothing to read");
        assert_eq!(std::fs::read_to_string(log).unwrap(), "nothing to read");
    }
}
