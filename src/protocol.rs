//! The socket wire format. This is the only module that spells request or
//! reply words; `cli` and `daemon` exchange domain values through it.
//!
//! One request line per connection, UTF-8, `\n`-terminated. The server
//! replies with one line and closes, except `follow`, which streams state
//! lines until the client hangs up.
//!
//! Frozen (plugin v1.2.2 depends on them): `follow`, the state words
//! `idle`/`speaking`/`error`. Changed in 1.0 (CLI and Daemon ship together):
//! `speak` carries an optional JSON object, so text with newlines or a
//! leading `--voice` survives the round trip.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use crate::exec::{CAPTURE_TIMEOUT, NOTIFY_TIMEOUT};
use crate::voices::VoiceName;

/// A request line longer than this is rejected, so one client cannot make the
/// Daemon buffer without bound. Inline text comes from argv, which is itself
/// capped near 2 MiB.
pub const MAX_REQUEST_BYTES: u64 = 1 << 20;

/// How long a connection may take to send its line before the Daemon drops it.
pub const READ_DEADLINE: Duration = Duration::from_secs(2);

pub const OK: &str = "ok";
pub const UNKNOWN: &str = "unknown command";

#[derive(Debug, PartialEq)]
pub enum Request {
    Follow,
    Speak(SpeakRequest),
    Stop,
    Status,
}

/// `speak` with no payload is a hotkey press (the Daemon resolves the Source).
/// `text` is raw: trimming and emptiness are decided by `speech::Text::new`.
#[derive(Debug, Default, PartialEq)]
pub struct SpeakRequest {
    pub text: Option<String>,
    pub voice: Option<VoiceName>,
}

/// Daemon states on the wire. The plugin matches these three words exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Speaking,
    Error,
}

impl State {
    pub fn wire(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Speaking => "speaking",
            State::Error => "error",
        }
    }
}

impl Request {
    /// Parses one request line (without its `\n`). `None` means the reply is
    /// `unknown command`, including a `speak` payload that is not a JSON
    /// object of optional `text` and `voice` strings.
    pub fn parse(line: &str) -> Option<Request> {
        match line.split_once(' ') {
            None => match line {
                "follow" => Some(Request::Follow),
                "stop" => Some(Request::Stop),
                "status" => Some(Request::Status),
                "speak" => Some(Request::Speak(SpeakRequest::default())),
                _ => None,
            },
            Some(("speak", payload)) => parse_speak(payload).map(Request::Speak),
            Some(_) => None,
        }
    }

    /// Inverse of `parse`. Never contains a raw `\n`.
    pub fn encode(&self) -> String {
        match self {
            Request::Follow => "follow".into(),
            Request::Stop => "stop".into(),
            Request::Status => "status".into(),
            Request::Speak(SpeakRequest {
                text: None,
                voice: None,
            }) => "speak".into(),
            Request::Speak(req) => {
                let mut obj = serde_json::Map::new();
                if let Some(text) = &req.text {
                    obj.insert("text".into(), text.as_str().into());
                }
                if let Some(voice) = &req.voice {
                    obj.insert("voice".into(), voice.as_str().into());
                }
                format!("speak {}", serde_json::Value::Object(obj))
            }
        }
    }
}

/// Unknown keys and non-string values are rejected. `voice` is checked for
/// syntax only; membership is the engine's call.
fn parse_speak(payload: &str) -> Option<SpeakRequest> {
    let obj: serde_json::Map<String, serde_json::Value> = serde_json::from_str(payload).ok()?;
    let mut req = SpeakRequest::default();
    for (key, value) in obj {
        let value = value.as_str()?;
        match key.as_str() {
            "text" => req.text = Some(value.to_owned()),
            "voice" => req.voice = Some(VoiceName::parse(value)?),
            _ => return None,
        }
    }
    Some(req)
}

/// The Daemon's slowest speak reply is a primary capture, a clipboard capture,
/// then a "nothing to read" notify, all before it writes `ok`. The margin
/// covers actor round trips.
pub const REPLY_DEADLINE: Duration = CAPTURE_TIMEOUT
    .saturating_mul(2)
    .saturating_add(NOTIFY_TIMEOUT)
    .saturating_add(Duration::from_secs(2));

/// The CLI side: one request, one reply line. Any connect/read failure is
/// `DaemonDown`, which the CLI turns into `daemon not running`.
pub fn request(socket: &Path, req: &Request) -> Result<String, DaemonDown> {
    let mut stream = UnixStream::connect(socket).map_err(|_| DaemonDown)?;
    stream
        .set_read_timeout(Some(REPLY_DEADLINE))
        .map_err(|_| DaemonDown)?;
    writeln!(stream, "{}", req.encode()).map_err(|_| DaemonDown)?;
    let mut line = String::new();
    BufReader::new(stream.take(4096))
        .read_line(&mut line)
        .map_err(|_| DaemonDown)?;
    if line.is_empty() {
        return Err(DaemonDown);
    }
    Ok(line.trim_end().to_owned())
}

#[derive(Debug)]
pub struct DaemonDown;

#[cfg(test)]
mod tests {
    use super::*;

    fn speak(text: Option<&str>, voice: Option<&str>) -> Request {
        Request::Speak(SpeakRequest {
            text: text.map(str::to_owned),
            voice: voice.map(|v| VoiceName::parse(v).unwrap()),
        })
    }

    #[test]
    fn reply_deadline_outlasts_the_daemons_slowest_speak() {
        assert!(REPLY_DEADLINE > CAPTURE_TIMEOUT * 2 + NOTIFY_TIMEOUT);
    }

    #[test]
    fn every_request_round_trips_on_one_line() {
        for req in [
            Request::Follow,
            Request::Stop,
            Request::Status,
            speak(None, None),
            speak(Some("line one\nline two"), None),
            speak(Some("--voice x"), None),
            speak(Some("-- --voice af_bella hi"), Some("af_bella")),
            speak(Some("naïve café — “quoted” \\n literal"), None),
            speak(Some(""), Some("bm_george")),
            speak(None, Some("af_bella")),
        ] {
            let line = req.encode();
            assert!(!line.contains('\n'), "{line:?}");
            assert_eq!(Request::parse(&line), Some(req), "{line:?}");
        }
    }

    #[test]
    fn frozen_words_are_byte_identical() {
        assert_eq!(Request::Follow.encode(), "follow");
        assert_eq!(Request::Stop.encode(), "stop");
        assert_eq!(Request::Status.encode(), "status");
        assert_eq!(speak(None, None).encode(), "speak");
        assert_eq!(
            speak(Some("Hi"), Some("af_bella")).encode(),
            r#"speak {"text":"Hi","voice":"af_bella"}"#
        );
    }

    #[test]
    fn bare_speak_and_empty_object_are_a_press() {
        assert_eq!(Request::parse("speak"), Some(speak(None, None)));
        assert_eq!(Request::parse("speak {}"), Some(speak(None, None)));
    }

    #[test]
    fn anything_else_is_unknown() {
        for line in [
            "frobnicate",
            "status extra",
            "follow now",
            "speak not-json",
            "speak Only inline text.",
            r#"speak {"text":1}"#,
            r#"speak {"text":"hi","extra":"x"}"#,
            r#"speak {"voice":"Not A Voice"}"#,
            r#"speak ["hi"]"#,
            "",
        ] {
            assert_eq!(Request::parse(line), None, "{line:?}");
        }
    }
}
