//! The Daemon: load, bind, accept, and the actor that owns every piece of
//! Utterance state.
//!
//! Threads: main (accept), one short-lived thread per request connection,
//! the actor, the synth thread (`stream::Synth`), and one playback thread per
//! Stream. Only the actor mutates `Phase` or the follower list; everyone else
//! sends it a `Command`. The actor never spawns a process and never does a
//! blocking write, so `status` and `follow` stay instant while a capture,
//! notify, or ORT run is slow.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::Sender;

use crate::config::{Config, Paths};
use crate::exec;
use crate::protocol::{self, Request, SpeakRequest};
use crate::speech::{self, Text};
use crate::stream::{REST_AFTER, SYNTH_THREAD, Synth};

mod actor;
#[cfg(test)]
mod tests;

use actor::{Actor, Command, Press, SpeakOutcome};

/// `omatalk daemon`. Loads and warms the engine, then binds: socket existence
/// means ready. Returns only on a startup error (printed by the caller, exit 1;
/// systemd retries).
pub fn serve(paths: &Paths) -> Result<std::convert::Infallible, String> {
    abort_on_panic_outside_synth();
    let engine = speech::load_engine(&paths.models, paths.fake_engine).map_err(|e| e.0)?;
    let listener = bind(paths)?;
    eprintln!("model warm");

    let (commands, inbox) = std::sync::mpsc::channel();
    let actor = Actor::new(Synth::spawn(engine, REST_AFTER), commands.clone());
    std::thread::Builder::new()
        .name("omatalk-actor".into())
        .spawn(move || actor.run(inbox))
        .map_err(|e| e.to_string())?;

    for conn in listener.incoming() {
        let Ok(conn) = conn else {
            // EMFILE fails instantly until a connection thread frees an fd;
            // without the pause the loop spins a core.
            std::thread::sleep(std::time::Duration::from_millis(10));
            continue;
        };
        let commands = commands.clone();
        let config_path = paths.config.clone();
        // Thread per request: a client that never sends `\n` costs one
        // sleeping thread for READ_DEADLINE, never the accept loop.
        let _ = std::thread::Builder::new()
            .name("omatalk-conn".into())
            .spawn(move || connection(conn, &commands, &config_path));
    }
    unreachable!("UnixListener::incoming never ends")
}

/// mkdir -p the parent; refuse if a live Daemon answers on the socket; unlink
/// a stale socket; bind.
fn bind(paths: &Paths) -> Result<UnixListener, String> {
    let socket = &paths.socket;
    let at = |e: std::io::Error| format!("{}: {e}", socket.display());
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir).map_err(at)?;
    }
    if UnixStream::connect(socket).is_ok() {
        return Err(format!("{}: daemon already running", socket.display()));
    }
    match std::fs::remove_file(socket) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(at(e)),
        _ => {}
    }
    UnixListener::bind(socket).map_err(at)
}

/// A panicked actor, connection, or playback thread may have left state half
/// changed, so the process aborts and systemd restarts it. The synth thread
/// is exempt: `stream.rs` catches each job's panic and reports it as an error.
fn abort_on_panic_outside_synth() {
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        report(info);
        if std::thread::current().name() != Some(SYNTH_THREAD) {
            std::process::abort();
        }
    }));
}

/// One request connection, start to finish, off the actor.
fn connection(conn: UnixStream, actor: &Sender<Command>, config_path: &std::path::Path) {
    let _ = conn.set_read_timeout(Some(protocol::READ_DEADLINE));
    let mut line = Vec::new();
    let read =
        BufReader::new((&conn).take(protocol::MAX_REQUEST_BYTES)).read_until(b'\n', &mut line);
    if read.is_err() || line.is_empty() {
        return;
    }
    let request = std::str::from_utf8(&line)
        .ok()
        .and_then(|l| Request::parse(l.trim_end_matches(['\n', '\r'])));
    let reply = match request {
        None => protocol::UNKNOWN.to_owned(),
        Some(Request::Follow) => {
            let _ = actor.send(Command::Follow(conn));
            return;
        }
        Some(Request::Status) => ask(actor, |reply| Command::Status { reply })
            .wire()
            .to_owned(),
        Some(Request::Stop) => {
            ask(actor, |reply| Command::Stop { reply });
            protocol::OK.to_owned()
        }
        Some(Request::Speak(req)) => {
            speak(actor, config_path, req);
            protocol::OK.to_owned()
        }
    };
    let mut conn = conn;
    let _ = writeln!(conn, "{reply}");
}

/// Reads config, resolves the press, asks the actor, and runs the "nothing to
/// read" notify before the reply (tests read the notify log after `ok`).
/// A malformed config.toml notifies `error: config.toml: ...` via the default
/// notify command and changes nothing.
fn speak(actor: &Sender<Command>, config_path: &std::path::Path, req: SpeakRequest) {
    let mut config = match Config::load(config_path) {
        Ok(c) => c,
        Err(e) => {
            exec::notify(&Config::defaults().notify, &format!("error: {e}"));
            return;
        }
    };
    if let Some(voice) = req.voice {
        config.voice = voice;
    }
    let first = req
        .text
        .as_deref()
        .and_then(Text::new)
        .or_else(|| exec::capture(&config.capture_primary));
    let send = |press| {
        ask(actor, |reply| Command::Speak {
            press,
            config: Box::new(config.clone()),
            reply,
        })
    };
    let outcome = match send(Press::First(first)) {
        SpeakOutcome::NeedClipboard => {
            send(Press::Clipboard(exec::capture(&config.capture_clipboard)))
        }
        other => other,
    };
    if outcome == SpeakOutcome::NothingToRead {
        exec::notify(&config.notify, "nothing to read");
    }
}

/// The actor lives as long as the process (its panic aborts), so it always
/// receives and replies.
fn ask<T>(actor: &Sender<Command>, make: impl FnOnce(Sender<T>) -> Command) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    actor.send(make(tx)).expect("actor lives for the process");
    rx.recv().expect("actor replies")
}
