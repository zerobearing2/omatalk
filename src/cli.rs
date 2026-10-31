//! argv to a `Cli` value, then run it. Hand-rolled: the surface is ten
//! commands, and the usage and error texts are contracts the panel shows
//! verbatim. Every stderr text lives in a `Display` impl next to its type.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{IsTerminal, Read};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::{Config, Paths, Setting, Speed};
use crate::protocol::{self, Request, SpeakRequest};
use crate::render;
use crate::speech::{Delivery, Text, Utterance};
use crate::voices::{self, VoiceName};

pub const USAGE: &str = "usage: omatalk {speak [--voice NAME] [TEXT...] | render --out PATH [--voice NAME] [--speed N] [TEXT...] | stop | status | version | \
config get [--json] | config set voice|speed VALUE | config voices [--json] | upgrade | uninstall}";

/// The Hyprland binding has no terminal, so a failed speak/stop also notifies.
pub const DAEMON_DOWN: &str = "daemon not running";
pub const DAEMON_DOWN_NOTIFY: &str = "daemon not running — systemctl --user start omatalk";

#[derive(Debug, PartialEq)]
pub enum Cli {
    Version,
    /// `text` is argv joined by single spaces; `None` when no words were given.
    /// `--voice` may appear anywhere before `--`; after `--` everything is text.
    Speak {
        text: Option<String>,
        voice: Option<String>,
    },
    /// Same word rules as `Speak`. `speed` stays raw so a bad one is a
    /// reported failure, not a usage error. `None` text reads stdin.
    Render {
        out: String,
        text: Option<String>,
        voice: Option<String>,
        speed: Option<String>,
    },
    Stop,
    Status,
    ConfigGet {
        json: bool,
    },
    ConfigSet {
        key: String,
        value: String,
    },
    ConfigVoices {
        json: bool,
    },
    Upgrade,
    Uninstall,
    /// The unit's ExecStart. Not in USAGE.
    Daemon,
}

/// Any argv `parse` rejects. Printed as USAGE; exit 2.
#[derive(Debug, PartialEq)]
pub struct Usage;

impl fmt::Display for Usage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(USAGE)
    }
}

/// `args` excludes argv[0].
pub fn parse(args: &[String]) -> Result<Cli, Usage> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["version"] | ["--version"] => Ok(Cli::Version),
        ["stop"] => Ok(Cli::Stop),
        ["status"] => Ok(Cli::Status),
        ["upgrade"] => Ok(Cli::Upgrade),
        ["uninstall"] => Ok(Cli::Uninstall),
        ["daemon"] => Ok(Cli::Daemon),
        ["config", "get"] => Ok(Cli::ConfigGet { json: false }),
        ["config", "get", "--json"] => Ok(Cli::ConfigGet { json: true }),
        ["config", "voices"] => Ok(Cli::ConfigVoices { json: false }),
        ["config", "voices", "--json"] => Ok(Cli::ConfigVoices { json: true }),
        ["config", "set", key, value] => Ok(Cli::ConfigSet {
            key: key.to_string(),
            value: value.to_string(),
        }),
        ["speak", rest @ ..] => parse_speak(rest),
        ["render", rest @ ..] => parse_render(rest),
        _ => Err(Usage),
    }
}

fn parse_speak(rest: &[&str]) -> Result<Cli, Usage> {
    let mut words = Vec::new();
    let mut voice = None;
    let mut it = rest.iter();
    while let Some(&word) = it.next() {
        match word {
            "--" => {
                words.extend(it.by_ref().copied());
                break;
            }
            "--voice" => voice = Some(it.next().ok_or(Usage)?.to_string()),
            _ if word.starts_with("--voice=") => voice = Some(word["--voice=".len()..].to_owned()),
            _ if word.starts_with("--") => return Err(Usage),
            _ => words.push(word),
        }
    }
    let text = (!words.is_empty()).then(|| words.join(" "));
    Ok(Cli::Speak { text, voice })
}

fn parse_render(rest: &[&str]) -> Result<Cli, Usage> {
    let mut words = Vec::new();
    let (mut out, mut voice, mut speed) = (None, None, None);
    let mut it = rest.iter();
    while let Some(&word) = it.next() {
        let flag = |name: &str| word.strip_prefix(name).and_then(|v| v.strip_prefix('='));
        match word {
            "--" => {
                words.extend(it.by_ref().copied());
                break;
            }
            "--out" => out = Some(it.next().ok_or(Usage)?.to_string()),
            "--voice" => voice = Some(it.next().ok_or(Usage)?.to_string()),
            "--speed" => speed = Some(it.next().ok_or(Usage)?.to_string()),
            _ if flag("--out").is_some() => out = flag("--out").map(str::to_owned),
            _ if flag("--voice").is_some() => voice = flag("--voice").map(str::to_owned),
            _ if flag("--speed").is_some() => speed = flag("--speed").map(str::to_owned),
            _ if word.starts_with("--") => return Err(Usage),
            _ => words.push(word),
        }
    }
    let text = (!words.is_empty()).then(|| words.join(" "));
    Ok(Cli::Render {
        out: out.ok_or(Usage)?,
        text,
        voice,
        speed,
    })
}

pub fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(cli) => run(cli, &Paths::from_env()),
        Err(usage) => {
            eprintln!("{usage}");
            ExitCode::from(2)
        }
    }
}

/// Exit codes: 0 success, 1 a reported failure (one stderr line), 2 usage.
pub fn run(cli: Cli, paths: &Paths) -> ExitCode {
    match cli {
        Cli::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Cli::Status => send(paths, &Request::Status),
        Cli::Stop => send(paths, &Request::Stop),
        Cli::Speak { text, voice } => {
            let voice = match voice.map(|v| voices::known(&paths.models, &v)).transpose() {
                Ok(v) => v,
                Err(unknown) => return fail(unknown),
            };
            send(paths, &Request::Speak(SpeakRequest { text, voice }))
        }
        Cli::Render {
            out,
            text,
            voice,
            speed,
        } => render(paths, &out, text, voice, speed),
        Cli::ConfigGet { json } => match Config::load(&paths.config) {
            Ok(c) if json => print_lines([c.to_json()]),
            Ok(c) => print_lines(c.to_lines()),
            Err(e) => fail(e),
        },
        Cli::ConfigVoices { json } => match voices::list(&paths.models) {
            Ok(names) if json => print_lines([voices_json(&names)]),
            Ok(names) => print_lines(names.iter().map(VoiceName::to_string)),
            Err(e) => fail(voices::VoiceError::Archive(e)),
        },
        Cli::ConfigSet { key, value } => match Setting::parse(&paths.models, &key, &value) {
            Ok(setting) => match setting.write(&paths.config) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => fail(format!("{}: {e}", paths.config.display())),
            },
            Err(e) => fail(e),
        },
        Cli::Upgrade => upgrade(paths),
        Cli::Uninstall => uninstall(),
        Cli::Daemon => match crate::daemon::serve(paths) {
            Ok(never) => match never {},
            Err(e) => fail(e),
        },
    }
}

/// One request; prints the reply. Daemon down: `daemon not running` on
/// stderr, rc 1, and (for speak/stop, not status) a desktop notify.
fn send(paths: &Paths, req: &Request) -> ExitCode {
    match protocol::request(&paths.socket, req) {
        Ok(reply) => print_lines([reply]),
        Err(protocol::DaemonDown) => {
            if *req != Request::Status {
                let argv = Config::load(&paths.config)
                    .unwrap_or_else(|_| Config::defaults())
                    .notify;
                crate::exec::notify(&argv, DAEMON_DOWN_NOTIFY);
            }
            fail(DAEMON_DOWN)
        }
    }
}

/// Everything but the synthesis is checked first, so a typo costs no model
/// load. Never talks to the Daemon. Speed is 1.0, not the config's: that is a
/// listening preference.
fn render(
    paths: &Paths,
    out: &str,
    text: Option<String>,
    voice: Option<String>,
    speed: Option<String>,
) -> ExitCode {
    let out = match render::check_out(out) {
        Ok(out) => out,
        Err(e) => return fail(e),
    };
    let speed = match speed.as_deref().map(Speed::parse).transpose() {
        Ok(speed) => speed.unwrap_or(Speed::NORMAL),
        Err(e) => return fail(e),
    };
    let voice = match voice {
        Some(raw) => raw,
        None => match Config::load(&paths.config) {
            Ok(config) => config.voice.to_string(),
            Err(e) => return fail(e),
        },
    };
    let voice = match voices::known(&paths.models, &voice) {
        Ok(voice) => voice,
        Err(unknown) => return fail(unknown),
    };
    let raw = match text {
        Some(words) => words,
        None => match read_stdin() {
            Ok(Some(piped)) => piped,
            Ok(None) => {
                eprintln!("{Usage}");
                return ExitCode::from(2);
            }
            Err(e) => return fail(format!("stdin: {e}")),
        },
    };
    let Some(text) = Text::new(&raw) else {
        return fail("nothing to read");
    };
    let utterance = Utterance {
        text,
        voice,
        speed,
        delivery: Delivery::Voiceover,
    };
    let tty = std::io::stderr().is_terminal();
    let progress = |batches: usize, audio: Duration| {
        if tty {
            eprint!("\rrendered {batches} batches, {:.1} s", audio.as_secs_f64());
        }
    };
    let result = render::render(paths, &utterance, &out, progress);
    if tty {
        eprintln!();
    }
    match result {
        Ok(audio) => print_lines([format!("{} {:.1} s", out.display(), audio.as_secs_f64())]),
        Err(e) => fail(e),
    }
}

/// All of stdin, or `None` when it is a terminal: waiting on a person's
/// keyboard is a usage mistake, not a script.
fn read_stdin() -> std::io::Result<Option<String>> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }
    let mut raw = String::new();
    stdin.lock().read_to_string(&mut raw)?;
    Ok(Some(raw))
}

/// `curl $SITE_BASE/install.sh?ts=<epoch>` (HTTPS only), then exec it with
/// the environment inherited.
fn upgrade(paths: &Paths) -> ExitCode {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let fetched = Command::new("curl")
        .args(["-L", "--fail", "--silent", "--show-error"])
        .args(["--proto", "=https", "--proto-redir", "=https", "--tlsv1.2"])
        .arg(format!("{}/install.sh?ts={epoch}", paths.site))
        .stderr(Stdio::inherit())
        .output();
    match fetched {
        Ok(out) if out.status.success() => {
            exec_script(OsString::from_vec(out.stdout), "omatalk upgrade")
        }
        Ok(out) => fail(format!("upgrade failed: curl {}", out.status)),
        Err(e) => fail(format!("upgrade failed: curl: {e}")),
    }
}

/// The embedded `uninstall.sh`: no dependency on files under OMATALK_HOME,
/// which it deletes.
fn uninstall() -> ExitCode {
    exec_script(include_str!("../uninstall.sh"), "omatalk uninstall")
}

/// Replaces this process with `bash -c script`, like the site's
/// `bash -c "$(curl ...)"`: stdin stays free for the script's prompts.
/// Returns only if exec failed.
fn exec_script(script: impl AsRef<OsStr>, label: &str) -> ExitCode {
    let err = Command::new("bash").arg("-c").arg(script).arg(label).exec();
    fail(format!("{label} failed: bash: {err}"))
}

fn voices_json(names: &[VoiceName]) -> String {
    serde_json::Value::from(names.iter().map(VoiceName::as_str).collect::<Vec<_>>()).to_string()
}

fn print_lines<S: AsRef<str>>(lines: impl IntoIterator<Item = S>) -> ExitCode {
    for line in lines {
        println!("{}", line.as_ref());
    }
    ExitCode::SUCCESS
}

fn fail(msg: impl fmt::Display) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::from(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(words: &[&str]) -> Result<Cli, Usage> {
        parse(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>())
    }

    fn speak(text: Option<&str>, voice: Option<&str>) -> Result<Cli, Usage> {
        Ok(Cli::Speak {
            text: text.map(str::to_owned),
            voice: voice.map(str::to_owned),
        })
    }

    #[test]
    fn speak_variants() {
        assert_eq!(p(&["speak"]), speak(None, None));
        assert_eq!(p(&["speak", "a", "b"]), speak(Some("a b"), None));
        assert_eq!(
            p(&["speak", "--voice", "v", "a"]),
            speak(Some("a"), Some("v"))
        );
        assert_eq!(
            p(&["speak", "a", "--voice", "v"]),
            speak(Some("a"), Some("v"))
        );
        assert_eq!(p(&["speak", "--voice=v", "a"]), speak(Some("a"), Some("v")));
        assert_eq!(
            p(&["speak", "--", "--voice", "x"]),
            speak(Some("--voice x"), None)
        );
        assert_eq!(
            p(&["speak", "--voice", "v", "--", "--bogus"]),
            speak(Some("--bogus"), Some("v"))
        );
        assert_eq!(
            p(&["speak", "-5", "degrees"]),
            speak(Some("-5 degrees"), None)
        );
        assert_eq!(
            p(&["speak", "line one\nline two"]),
            speak(Some("line one\nline two"), None)
        );
    }

    #[test]
    fn speak_usage_errors() {
        assert_eq!(p(&["speak", "--voice"]), Err(Usage));
        assert_eq!(p(&["speak", "--bogus"]), Err(Usage));
        assert_eq!(p(&["speak", "hi", "--bogus", "there"]), Err(Usage));
    }

    fn render(
        out: &str,
        text: Option<&str>,
        voice: Option<&str>,
        speed: Option<&str>,
    ) -> Result<Cli, Usage> {
        Ok(Cli::Render {
            out: out.to_owned(),
            text: text.map(str::to_owned),
            voice: voice.map(str::to_owned),
            speed: speed.map(str::to_owned),
        })
    }

    #[test]
    fn render_variants() {
        assert_eq!(
            p(&["render", "--out", "a.wav"]),
            render("a.wav", None, None, None)
        );
        assert_eq!(
            p(&["render", "--out=a.wav", "one", "two"]),
            render("a.wav", Some("one two"), None, None)
        );
        assert_eq!(
            p(&[
                "render",
                "hi",
                "--speed",
                "1.2",
                "--voice=v",
                "--out",
                "a.wav"
            ]),
            render("a.wav", Some("hi"), Some("v"), Some("1.2"))
        );
        assert_eq!(
            p(&[
                "render",
                "--speed=0.9",
                "--out",
                "a.wav",
                "--",
                "--out",
                "x"
            ]),
            render("a.wav", Some("--out x"), None, Some("0.9"))
        );
        assert_eq!(
            p(&["render", "--out", "a.wav", "line one\n\nline two"]),
            render("a.wav", Some("line one\n\nline two"), None, None)
        );
        assert_eq!(
            p(&["render", "--out", "a.wav", "-5", "degrees"]),
            render("a.wav", Some("-5 degrees"), None, None)
        );
    }

    #[test]
    fn render_usage_errors() {
        assert_eq!(p(&["render"]), Err(Usage));
        assert_eq!(p(&["render", "hello"]), Err(Usage));
        assert_eq!(p(&["render", "--", "--out", "a.wav"]), Err(Usage));
        for flag in ["--out", "--voice", "--speed"] {
            assert_eq!(p(&["render", "--out", "a.wav", flag]), Err(Usage), "{flag}");
        }
        assert_eq!(p(&["render", "--out", "a.wav", "--bogus"]), Err(Usage));
        assert_eq!(p(&["render", "--out", "a.wav", "--loud=1"]), Err(Usage));
        assert!(USAGE.contains("render --out PATH"));
    }

    #[test]
    fn fixed_commands() {
        assert_eq!(p(&["version"]), Ok(Cli::Version));
        assert_eq!(p(&["--version"]), Ok(Cli::Version));
        assert_eq!(p(&["stop"]), Ok(Cli::Stop));
        assert_eq!(p(&["status"]), Ok(Cli::Status));
        assert_eq!(p(&["daemon"]), Ok(Cli::Daemon));
        assert_eq!(
            p(&["config", "get", "--json"]),
            Ok(Cli::ConfigGet { json: true })
        );
        assert_eq!(
            p(&["config", "voices"]),
            Ok(Cli::ConfigVoices { json: false })
        );
        assert_eq!(
            p(&["config", "set", "speed", "1"]),
            Ok(Cli::ConfigSet {
                key: "speed".into(),
                value: "1".into()
            })
        );
    }

    #[test]
    fn extra_or_missing_words_are_usage() {
        for words in [
            &[][..],
            &["frob"],
            &["upgrade", "now"],
            &["uninstall", "now"],
            &["status", "--follow"],
            &["version", "x"],
            &["config"],
            &["config", "set", "speed"],
            &["config", "get", "--yaml"],
            &["daemon", "--foreground"],
        ] {
            assert_eq!(p(words), Err(Usage), "{words:?}");
        }
        assert!(Usage.to_string().starts_with("usage: omatalk "));
    }
}
