//! Pausing MPRIS media players while speech plays, and resuming only the
//! players that were paused here. Runs on its own thread because each player
//! is a busctl round trip, and the actor never spawns a process.

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crate::exec::{self, Argv};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

/// A hung player must not hold the others paused, or resumed, for long.
const CALL_TIMEOUT: Duration = Duration::from_millis(500);

enum Edge {
    /// Speech started: pause every playing player via this busctl.
    Pause(Argv),
    /// Speech ended: resume what the last `Pause` paused.
    Resume,
}

pub struct Media(Sender<Edge>);

impl Media {
    pub fn spawn() -> Media {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("omatalk-media".into())
            .spawn(move || run(rx))
            .expect("spawn media thread");
        Media(tx)
    }

    pub fn pause(&self, bus: Argv) {
        let _ = self.0.send(Edge::Pause(bus));
    }

    pub fn resume(&self) {
        let _ = self.0.send(Edge::Resume);
    }
}

fn run(edges: Receiver<Edge>) {
    let mut paused: Option<(Argv, Vec<String>)> = None;
    for edge in edges {
        match edge {
            Edge::Pause(bus) => {
                let names = players(&bus)
                    .into_iter()
                    .filter(|name| status(&bus, name).as_deref() == Some("Playing"))
                    .filter(|name| call(&bus, name, "Pause"))
                    .collect();
                paused = Some((bus, names));
            }
            Edge::Resume => {
                if let Some((bus, names)) = paused.take() {
                    for name in names {
                        call(&bus, &name, "Play");
                    }
                }
            }
        }
    }
}

fn busctl(bus: &Argv, args: &[&str]) -> Option<String> {
    let mut cmd = bus.command();
    cmd.arg(format!("--timeout={}", CALL_TIMEOUT.as_secs_f32()))
        .args(args);
    exec::stdout(cmd, Instant::now() + CALL_TIMEOUT * 2)
}

/// Only acquired names: an activatable player that is not running must not
/// be started by the status query.
fn players(bus: &Argv) -> Vec<String> {
    busctl(bus, &["--acquired", "--no-legend", "list"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| name.starts_with(PREFIX))
        .map(str::to_owned)
        .collect()
}

/// busctl prints a string property as `s "Playing"`.
fn status(bus: &Argv, name: &str) -> Option<String> {
    let out = busctl(bus, &["get-property", name, PATH, PLAYER, "PlaybackStatus"])?;
    Some(out.trim().strip_prefix("s ")?.trim_matches('"').to_owned())
}

fn call(bus: &Argv, name: &str, method: &str) -> bool {
    busctl(bus, &["call", name, PATH, PLAYER, method]).is_some()
}
