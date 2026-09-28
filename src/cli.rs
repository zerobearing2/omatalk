//! argv to a `Cli` value, then run it. Hand-rolled: the surface is ten
//! commands, and the usage and error texts are contracts the panel shows
//! verbatim. Every stderr text lives in a `Display` impl next to its type.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{Config, Paths, Setting};
use crate::protocol::{self, Request, SpeakRequest};
use crate::voices::{self, VoiceName};

pub const USAGE: &str = "usage: omatalk {speak [--voice NAME] [TEXT...] | stop | status | version | \
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
