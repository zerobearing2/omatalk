//! argv to a `Cli` value, then run it. clap owns help, usage, and argv
//! errors (exit 2). Every other stderr text lives in a `Display` impl next to
//! its type.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{IsTerminal, Read};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{CommandFactory, Parser, Subcommand};

use crate::config::{Config, Paths, Setting, Speed};
use crate::protocol::{self, Request, SpeakRequest};
use crate::render;
use crate::speech::{Delivery, Text, Utterance};
use crate::voices::{self, VoiceName};

/// The Hyprland binding has no terminal, so a failed speak/stop also notifies.
pub const DAEMON_DOWN: &str = "daemon not running";
pub const DAEMON_DOWN_NOTIFY: &str = "daemon not running — systemctl --user start omatalk";

#[derive(Debug, PartialEq, Parser)]
#[command(
    name = "omatalk",
    version,
    about = "Local text-to-speech for Omarchy",
    arg_required_else_help = true
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Cli,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Cli {
    /// Speak TEXT, or the selected text when none is given
    #[command(allow_negative_numbers = true)]
    Speak {
        /// Voice for this request only (see `omatalk config voices`)
        #[arg(long, value_name = "NAME")]
        voice: Option<String>,
        /// Words to speak; use `--` before text that starts with `-`
        #[arg(value_name = "TEXT")]
        text: Vec<String>,
    },
    /// Render TEXT, or stdin, to a WAV file
    #[command(
        allow_negative_numbers = true,
        after_help = "Examples:\n  omatalk render --out hello.wav \"Hello there.\"\n  omatalk render --out script.wav < script.txt"
    )]
    Render {
        /// WAV file to write
        #[arg(long, value_name = "PATH")]
        out: String,
        /// Voice [default: the configured voice]
        #[arg(long, value_name = "NAME")]
        voice: Option<String>,
        /// Speed, 0.5 to 2.0 [default: 1.0]
        #[arg(long, value_name = "N")]
        speed: Option<String>,
        /// Words to render; read from stdin when omitted
        #[arg(value_name = "TEXT")]
        text: Vec<String>,
    },
    /// Stop speaking
    Stop,
    /// Print the Daemon's state
    Status,
    /// Print the version
    Version,
    /// Read or change settings
    #[command(subcommand, arg_required_else_help = true)]
    Config(ConfigCommand),
    /// Install the latest release
    Upgrade,
    /// Remove Omatalk from this machine
    Uninstall,
    /// Run the Daemon (started by omatalk.service)
    #[command(hide = true)]
    Daemon,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum ConfigCommand {
    /// Print the settings
    Get {
        /// Print one JSON object
        #[arg(long)]
        json: bool,
    },
    /// Change one setting
    Set {
        /// Setting to change
        #[arg(value_parser = ["voice", "speed"])]
        key: String,
        /// A voice name, or a speed from 0.5 to 2.0
        value: String,
    },
    /// List the installed voices
    Voices {
        /// Print one JSON array
        #[arg(long)]
        json: bool,
    },
}

pub fn main() -> ExitCode {
    run(Args::parse().command, &Paths::from_env())
}

/// Words joined by single spaces; `None` when no words were given.
fn joined(words: Vec<String>) -> Option<String> {
    (!words.is_empty()).then(|| words.join(" "))
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
            let text = joined(text);
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
        } => render(paths, &out, joined(text), voice, speed),
        Cli::Config(ConfigCommand::Get { json }) => match Config::load(&paths.config) {
            Ok(c) if json => print_lines([c.to_json()]),
            Ok(c) => print_lines(c.to_lines()),
            Err(e) => fail(e),
        },
        Cli::Config(ConfigCommand::Voices { json }) => match voices::list(&paths.models) {
            Ok(names) if json => print_lines([voices_json(&names)]),
            Ok(names) => print_lines(names.iter().map(VoiceName::to_string)),
            Err(e) => fail(voices::VoiceError::Archive(e)),
        },
        Cli::Config(ConfigCommand::Set { key, value }) => {
            match Setting::parse(&paths.models, &key, &value) {
                Ok(setting) => match setting.write(&paths.config) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(e) => fail(format!("{}: {e}", paths.config.display())),
                },
                Err(e) => fail(e),
            }
        }
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
/// load. Never talks to the Daemon.
fn render(
    paths: &Paths,
    out: &str,
    text: Option<String>,
    voice: Option<String>,
    speed: Option<String>,
) -> ExitCode {
    let (out, utterance) = match render_inputs(paths, out, text, voice, speed) {
        Ok(inputs) => inputs,
        Err(code) => return code,
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

/// The target and the Utterance, or the exit code of the first check that
/// failed. Speed is 1.0, not the config's: that is a listening preference.
fn render_inputs(
    paths: &Paths,
    out: &str,
    text: Option<String>,
    voice: Option<String>,
    speed: Option<String>,
) -> Result<(PathBuf, Utterance), ExitCode> {
    let out = render::check_out(out).map_err(fail)?;
    let speed = speed
        .as_deref()
        .map(Speed::parse)
        .transpose()
        .map_err(fail)?;
    let voice = render_voice(paths, voice)?;
    let text = render_text(text)?;
    let utterance = Utterance {
        text,
        voice,
        speed: speed.unwrap_or(Speed::NORMAL),
        delivery: Delivery::Voiceover,
    };
    Ok((out, utterance))
}

/// `--voice`, else the configured one; either must be in the archive.
fn render_voice(paths: &Paths, voice: Option<String>) -> Result<VoiceName, ExitCode> {
    let Some(raw) = voice else {
        let configured = Config::load(&paths.config).map_err(fail)?.voice;
        voices::in_archive(&paths.models, &configured).map_err(fail)?;
        return Ok(configured);
    };
    voices::known(&paths.models, &raw).map_err(fail)
}

/// The words, else stdin; blank is nothing to read, and a terminal is a
/// usage error.
fn render_text(words: Option<String>) -> Result<Text, ExitCode> {
    let raw = match words {
        Some(words) => words,
        None => match read_stdin() {
            Ok(Some(piped)) => piped,
            Ok(None) => Args::command()
                .bin_name("omatalk render")
                .override_usage("omatalk render --out <PATH> [TEXT]...")
                .error(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "no TEXT given and stdin is a terminal",
                )
                .exit(),
            Err(e) => return Err(fail(format!("stdin: {e}"))),
        },
    };
    Text::new(&raw).ok_or_else(|| fail("nothing to read"))
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
    use clap::error::ErrorKind;

    fn p(words: &[&str]) -> Result<Cli, ErrorKind> {
        Args::try_parse_from(std::iter::once("omatalk").chain(words.iter().copied()))
            .map(|args| args.command)
            .map_err(|e| e.kind())
    }

    fn words(text: &str) -> Vec<String> {
        text.split(' ').map(str::to_owned).collect()
    }

    fn speak(text: &str, voice: Option<&str>) -> Result<Cli, ErrorKind> {
        Ok(Cli::Speak {
            text: if text.is_empty() { vec![] } else { words(text) },
            voice: voice.map(str::to_owned),
        })
    }

    #[test]
    fn definition_is_valid() {
        Args::command().debug_assert();
    }

    #[test]
    fn speak_variants() {
        assert_eq!(p(&["speak"]), speak("", None));
        assert_eq!(p(&["speak", "a", "b"]), speak("a b", None));
        assert_eq!(p(&["speak", "--voice", "v", "a"]), speak("a", Some("v")));
        assert_eq!(p(&["speak", "a", "--voice", "v"]), speak("a", Some("v")));
        assert_eq!(p(&["speak", "--voice=v", "a"]), speak("a", Some("v")));
        assert_eq!(
            p(&["speak", "--", "--voice", "x"]),
            speak("--voice x", None)
        );
        assert_eq!(
            p(&["speak", "--voice", "v", "--", "--bogus"]),
            speak("--bogus", Some("v"))
        );
        assert_eq!(p(&["speak", "-5", "degrees"]), speak("-5 degrees", None));
        assert_eq!(
            p(&["speak", "line one\nline two"]),
            Ok(Cli::Speak {
                text: vec!["line one\nline two".into()],
                voice: None
            })
        );
        assert_eq!(joined(words("a b")), Some("a b".into()));
        assert_eq!(joined(vec![]), None);
    }

    #[test]
    fn speak_usage_errors() {
        assert_eq!(p(&["speak", "--voice"]), Err(ErrorKind::InvalidValue));
        assert_eq!(p(&["speak", "--bogus"]), Err(ErrorKind::UnknownArgument));
        assert_eq!(
            p(&["speak", "hi", "--bogus", "there"]),
            Err(ErrorKind::UnknownArgument)
        );
    }

    fn render(
        out: &str,
        text: &[&str],
        voice: Option<&str>,
        speed: Option<&str>,
    ) -> Result<Cli, ErrorKind> {
        Ok(Cli::Render {
            out: out.to_owned(),
            text: text.iter().map(|w| w.to_string()).collect(),
            voice: voice.map(str::to_owned),
            speed: speed.map(str::to_owned),
        })
    }

    #[test]
    fn render_variants() {
        assert_eq!(
            p(&["render", "--out", "a.wav"]),
            render("a.wav", &[], None, None)
        );
        assert_eq!(
            p(&["render", "--out=a.wav", "one", "two"]),
            render("a.wav", &["one", "two"], None, None)
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
            render("a.wav", &["hi"], Some("v"), Some("1.2"))
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
            render("a.wav", &["--out", "x"], None, Some("0.9"))
        );
        assert_eq!(
            p(&["render", "--out", "a.wav", "-5", "degrees"]),
            render("a.wav", &["-5", "degrees"], None, None)
        );
        assert_eq!(
            p(&["render", "--out", "a.wav", "--speed", "-1"]),
            render("a.wav", &[], None, Some("-1"))
        );
    }

    #[test]
    fn render_usage_errors() {
        use ErrorKind::*;
        assert_eq!(p(&["render"]), Err(MissingRequiredArgument));
        assert_eq!(p(&["render", "hello"]), Err(MissingRequiredArgument));
        assert_eq!(
            p(&["render", "--", "--out", "a.wav"]),
            Err(MissingRequiredArgument)
        );
        for flag in ["--out", "--voice", "--speed"] {
            assert_eq!(
                p(&["render", "--out", "a.wav", flag]),
                Err(InvalidValue),
                "{flag}"
            );
        }
        assert_eq!(
            p(&["render", "--out", "a.wav", "--bogus"]),
            Err(UnknownArgument)
        );
        assert_eq!(
            p(&["render", "--out", "a.wav", "--loud=1"]),
            Err(UnknownArgument)
        );
    }

    #[test]
    fn fixed_commands() {
        assert_eq!(p(&["version"]), Ok(Cli::Version));
        assert_eq!(p(&["stop"]), Ok(Cli::Stop));
        assert_eq!(p(&["status"]), Ok(Cli::Status));
        assert_eq!(p(&["daemon"]), Ok(Cli::Daemon));
        assert_eq!(
            p(&["config", "get", "--json"]),
            Ok(Cli::Config(ConfigCommand::Get { json: true }))
        );
        assert_eq!(
            p(&["config", "voices"]),
            Ok(Cli::Config(ConfigCommand::Voices { json: false }))
        );
        assert_eq!(
            p(&["config", "set", "speed", "1"]),
            Ok(Cli::Config(ConfigCommand::Set {
                key: "speed".into(),
                value: "1".into()
            }))
        );
    }

    #[test]
    fn help_and_version_are_not_errors_of_use() {
        for words in [
            &["--help"][..],
            &["-h"],
            &["help"],
            &["help", "render"],
            &["render", "--help"],
        ] {
            assert_eq!(p(words), Err(ErrorKind::DisplayHelp), "{words:?}");
        }
        for words in [&["--version"][..], &["-V"]] {
            assert_eq!(p(words), Err(ErrorKind::DisplayVersion), "{words:?}");
        }
        assert_eq!(
            p(&[]),
            Err(ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand)
        );
    }

    #[test]
    fn extra_or_missing_words_are_usage() {
        for words in [
            &["frob"][..],
            &["upgrade", "now"],
            &["uninstall", "now"],
            &["status", "--follow"],
            &["version", "x"],
            &["config", "set", "speed"],
            &["config", "set", "pitch", "1"],
            &["config", "get", "--yaml"],
            &["daemon", "--foreground"],
        ] {
            assert!(p(words).is_err(), "{words:?}");
        }
    }
}
