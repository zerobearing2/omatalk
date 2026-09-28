//! Paths from the environment and the typed config.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use toml_edit::{DocumentMut, Item, Value};

use crate::exec::Argv;
use crate::voices::VoiceName;

pub const SITE_BASE: &str = "https://omatalk.zerobearing.com";

/// Resolved once per process. Every Omatalk env var is read here; the only
/// other env read is ort's own `ORT_DYLIB_PATH`, in `speech::kokoro`.
pub struct Paths {
    /// `$OMATALK_SOCKET`, else `${XDG_RUNTIME_DIR:-/run/user/$UID}/omatalk/omatalk.sock`.
    /// The plugin computes the same path independently.
    pub socket: PathBuf,
    /// `$OMATALK_CONFIG`, else `~/.config/omatalk/config.toml`.
    pub config: PathBuf,
    /// `$OMATALK_MODELS`, else `~/.local/share/omatalk/models`.
    pub models: PathBuf,
    /// `$SITE_BASE` (trailing `/` stripped), else `SITE_BASE`.
    pub site: String,
    /// `OMATALK_TEST_FAKE_ENGINE` set: the Daemon skips model, lexicon, and
    /// dlopen, and synthesizes silence. The black-box tests' seam.
    pub fake_engine: bool,
}

impl Paths {
    pub fn from_env() -> Paths {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let home = var("HOME").unwrap_or_default();
        // SAFETY: getuid has no preconditions and cannot fail.
        let runtime = var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|| format!("/run/user/{}", unsafe { libc::getuid() }).into());
        Paths {
            socket: var("OMATALK_SOCKET").unwrap_or_else(|| runtime.join("omatalk/omatalk.sock")),
            config: var("OMATALK_CONFIG")
                .unwrap_or_else(|| home.join(".config/omatalk/config.toml")),
            models: var("OMATALK_MODELS")
                .unwrap_or_else(|| home.join(".local/share/omatalk/models")),
            site: var("SITE_BASE")
                .map(|v| v.to_string_lossy().trim_end_matches('/').to_owned())
                .unwrap_or_else(|| SITE_BASE.to_owned()),
            fake_engine: var("OMATALK_TEST_FAKE_ENGINE").is_some(),
        }
    }
}

/// The effective config: defaults overlaid with config.toml. Re-read at every
/// Utterance start (by the connection thread, never by the actor).
#[derive(Clone, Debug)]
pub struct Config {
    pub voice: VoiceName,
    pub speed: Speed,
    pub capture_primary: Argv,
    pub capture_clipboard: Argv,
    /// pw-cat-compatible: raw s16le mono on stdin. The Daemon appends
    /// `--rate 24000 --channels 1 -`.
    pub player: Argv,
    /// The message is appended as the last argument.
    pub notify: Argv,
    /// pactl-compatible. The Daemon appends `get-default-sink`, then
    /// `list sinks short`.
    pub sink_probe: Argv,
    /// Silence played before speech when the default sink is a suspended
    /// Bluetooth or HDMI sink. Zero disables it.
    pub wake_lead: Duration,
}

impl Config {
    /// `tests/site.rs` pins voice and speed against the site snippet, and
    /// `tests/cli.rs` pins every key against the README Config table.
    pub fn defaults() -> Config {
        let argv = |words: &[&str]| {
            Argv::new(words.iter().map(|w| w.to_string()).collect()).expect("non-empty")
        };
        Config {
            voice: VoiceName::parse("af_heart").expect("valid default voice"),
            speed: Speed(1.0),
            capture_primary: argv(&["wl-paste", "--primary"]),
            capture_clipboard: argv(&["wl-paste"]),
            player: argv(&["pw-cat", "-p", "--raw", "--format", "s16"]),
            notify: argv(&["notify-send", "Omatalk"]),
            sink_probe: argv(&["pactl"]),
            wake_lead: Duration::from_millis(650),
        }
    }

    /// A missing file is the defaults. Unknown keys are ignored (hand-edited
    /// files survive upgrades). A malformed file or a bad value is an error;
    /// it never crashes the Daemon (a known Python bug).
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let doc = match read_document(path) {
            Ok(Some(doc)) => doc,
            Ok(None) => return Ok(Config::defaults()),
            Err(e) => return Err(ConfigError(e.to_string())),
        };
        let mut config = Config::defaults();
        for (key, item) in doc.iter() {
            let bad = |what: &str| ConfigError(format!("{key} must be {what}"));
            match key {
                "voice" => {
                    config.voice = item
                        .as_str()
                        .and_then(VoiceName::parse)
                        .ok_or_else(|| bad("a voice name"))?;
                }
                "speed" => {
                    let raw = match (item.as_float(), item.as_integer()) {
                        (Some(f), _) => f as f32,
                        (_, Some(i)) => i as f32,
                        _ => return Err(bad("a number")),
                    };
                    config.speed = Speed::new(raw).ok_or_else(|| bad(Speed::RANGE))?;
                }
                "lang" => {
                    item.as_str().ok_or_else(|| bad("a string"))?;
                }
                "wake_lead_ms" => {
                    config.wake_lead = item
                        .as_integer()
                        .filter(|ms| (0..=WAKE_LEAD_MAX_MS).contains(ms))
                        .map(|ms| Duration::from_millis(ms as u64))
                        .ok_or_else(|| {
                            bad(&format!("an integer between 0 and {WAKE_LEAD_MAX_MS}"))
                        })?;
                }
                "capture_primary" | "capture_clipboard" | "player" | "notify" | "sink_probe" => {
                    let argv = item
                        .as_array()
                        .and_then(|a| {
                            a.iter()
                                .map(|v| v.as_str().map(str::to_owned))
                                .collect::<Option<Vec<_>>>()
                        })
                        .and_then(Argv::new)
                        .ok_or_else(|| bad("a non-empty list of strings"))?;
                    match key {
                        "capture_primary" => config.capture_primary = argv,
                        "capture_clipboard" => config.capture_clipboard = argv,
                        "player" => config.player = argv,
                        "notify" => config.notify = argv,
                        _ => config.sink_probe = argv,
                    }
                }
                _ => {}
            }
        }
        Ok(config)
    }

    /// `config get --json`: one line, every key, speed as a JSON number.
    pub fn to_json(&self) -> String {
        let map = self
            .entries()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        serde_json::Value::Object(map).to_string()
    }

    /// `config get`: `key = value` lines sorted by key.
    pub fn to_lines(&self) -> Vec<String> {
        let mut entries = self.entries();
        entries.sort_by_key(|(k, _)| *k);
        entries
            .into_iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) => format!("{k} = {s}"),
                other => format!("{k} = {other}"),
            })
            .collect()
    }

    fn entries(&self) -> Vec<(&'static str, serde_json::Value)> {
        let argv = |a: &Argv| serde_json::Value::from(a.words().collect::<Vec<_>>());
        vec![
            ("voice", self.voice.as_str().into()),
            ("speed", self.speed.as_f64().into()),
            ("lang", LANG.into()),
            ("capture_primary", argv(&self.capture_primary)),
            ("capture_clipboard", argv(&self.capture_clipboard)),
            ("player", argv(&self.player)),
            ("notify", argv(&self.notify)),
            ("sink_probe", argv(&self.sink_probe)),
            ("wake_lead_ms", (self.wake_lead.as_millis() as u64).into()),
        ]
    }
}

/// Measured on a Bluetooth speaker: 600 ms of lead is clean, 400 ms clips; the 650 default adds margin.
/// Past two seconds the lead is a delay nobody wants.
const WAKE_LEAD_MAX_MS: i64 = 2000;

/// Utterance speed multiplier, `Speed::MIN` to `Speed::MAX` inclusive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speed(f32);

impl Speed {
    pub const MIN: f32 = 0.5;
    pub const MAX: f32 = 2.0;
    /// `MIN` and `MAX` as the error texts spell them.
    const RANGE: &str = "between 0.5 and 2.0";

    /// Accepts `1`, `1.0`, `1.25`. Error Display is the exact stderr line.
    pub fn parse(raw: &str) -> Result<Speed, SetError> {
        let value: f32 = raw
            .trim()
            .parse()
            .map_err(|_| SetError::SpeedNotNumber(raw.to_owned()))?;
        Speed::new(value).ok_or_else(|| SetError::SpeedOutOfRange(raw.to_owned()))
    }

    fn new(value: f32) -> Option<Speed> {
        (Self::MIN..=Self::MAX)
            .contains(&value)
            .then_some(Speed(value))
    }

    pub fn get(self) -> f32 {
        self.0
    }

    /// TOML and JSON store f64; go through the f32's shortest decimal so 1.1
    /// is written `1.1`, not `1.100000023841858`.
    fn as_f64(self) -> f64 {
        self.0
            .to_string()
            .parse()
            .expect("f32 Display parses as f64")
    }
}

/// The only lexicon the misaki port carries. Any `lang` string loads as
/// en-us: a hand-edited `lang = "en-gb"` from v0.5 must not silence every
/// press, and British text through the US lexicon still reads correctly.
const LANG: &str = "en-us";

/// One accepted `config set`. Only these two keys are settable; everything
/// else is edited in config.toml by hand.
pub enum Setting {
    Voice(VoiceName),
    Speed(Speed),
}

impl Setting {
    /// Parses `config set <key> <value>`; `voice` is checked against the
    /// voices archive via `voices::known`.
    pub fn parse(models: &Path, key: &str, raw: &str) -> Result<Setting, SetError> {
        match key {
            "voice" => crate::voices::known(models, raw)
                .map(Setting::Voice)
                .map_err(SetError::Voice),
            "speed" => Speed::parse(raw).map(Setting::Speed),
            other => Err(SetError::NotSettable(other.to_owned())),
        }
    }

    /// Writes exactly this key into config.toml. Every other byte (comments,
    /// ordering, keys it did not set) is preserved; a key left at its default
    /// is never written. Creates the file and its parent if missing. Atomic:
    /// temp file + rename, keeping the old file's mode. Speed is always
    /// written as a float (`1.0`).
    pub fn write(&self, path: &Path) -> io::Result<()> {
        let path = link_target(path)?;
        let mut doc = read_document(&path)?.unwrap_or_default();
        let (key, mut value) = match self {
            Setting::Voice(v) => ("voice", Value::from(v.as_str())),
            Setting::Speed(s) => ("speed", Value::from(s.as_f64())),
        };
        if let Some(old) = doc.get(key).and_then(Item::as_value) {
            *value.decor_mut() = old.decor().clone();
        }
        doc[key] = Item::Value(value);

        let dir = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir)?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
        fs::write(&tmp, doc.to_string())?;
        let kept_mode = match fs::metadata(&path) {
            Ok(old) => fs::set_permissions(&tmp, old.permissions()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        };
        kept_mode
            .and_then(|()| fs::rename(&tmp, &path))
            .inspect_err(|_| {
                let _ = fs::remove_file(&tmp);
            })
    }
}

/// Where a write must land so a dotfiles symlink stays a symlink: `path` with
/// every symlink followed, even when the final target does not exist yet.
fn link_target(path: &Path) -> io::Result<PathBuf> {
    let mut path = path.to_owned();
    for _ in 0..40 {
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = fs::read_link(&path)?;
                path = path.parent().unwrap_or(Path::new("")).join(target);
            }
            _ => return Ok(path),
        }
    }
    Err(io::Error::from_raw_os_error(libc::ELOOP))
}

/// `None` when the file does not exist.
fn read_document(path: &Path) -> io::Result<Option<DocumentMut>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    text.parse::<DocumentMut>()
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.message().to_owned()))
}

/// Every `config set` failure; Display is the one stderr line the panel shows.
#[derive(Debug)]
pub enum SetError {
    Voice(crate::voices::VoiceError),
    SpeedNotNumber(String),
    SpeedOutOfRange(String),
    NotSettable(String),
}

impl fmt::Display for SetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetError::Voice(e) => e.fmt(f),
            SetError::SpeedNotNumber(r) => write!(f, "{r}: speed must be a number"),
            SetError::SpeedOutOfRange(r) => write!(f, "{r}: speed must be {}", Speed::RANGE),
            SetError::NotSettable(k) => {
                write!(
                    f,
                    "{k}: not settable via config set (edit config.toml directly)"
                )
            }
        }
    }
}

/// Display is the notify text after `error: `, e.g. `config.toml: speed must be between 0.5 and 2.0`.
#[derive(Debug)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "config.toml: {}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn speed_parse_accepts_integer_and_float() {
        assert_eq!(Speed::parse("1").unwrap().get(), 1.0);
        assert_eq!(Speed::parse("1.0").unwrap().get(), 1.0);
        assert_eq!(Speed::parse("0.5").unwrap().get(), 0.5);
        assert_eq!(Speed::parse("2").unwrap().get(), 2.0);
        assert_eq!(Speed::parse("1.25").unwrap().get(), 1.25);
    }

    #[test]
    fn speed_parse_errors_are_the_stderr_lines() {
        assert_eq!(
            Speed::parse("fast").unwrap_err().to_string(),
            "fast: speed must be a number"
        );
        assert_eq!(
            Speed::parse("2.1").unwrap_err().to_string(),
            "2.1: speed must be between 0.5 and 2.0"
        );
        assert_eq!(
            Speed::parse("0.4").unwrap_err().to_string(),
            "0.4: speed must be between 0.5 and 2.0"
        );
        assert_eq!(
            Speed::parse("nan").unwrap_err().to_string(),
            "nan: speed must be between 0.5 and 2.0"
        );
    }

    #[test]
    fn set_speed_preserves_other_keys_comments_and_order() {
        let dir = TempDir::new("config-set");
        let path = dir.path().join("config.toml");
        let before = "# my omatalk\nvoice = \"af_bella\" # favourite\n\n# slower please\nspeed = 0.9 # note\nplayer = [\"custom\", \"--flag\"]\nunknown_key = true\n";
        fs::write(&path, before).unwrap();

        Setting::Speed(Speed::parse("1").unwrap())
            .write(&path)
            .unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            before.replace("speed = 0.9", "speed = 1.0")
        );
    }

    #[test]
    fn set_voice_appends_only_that_key_to_a_missing_file() {
        let dir = TempDir::new("config-new");
        let path = dir.path().join("nested/config.toml");
        Setting::Voice(VoiceName::parse("af_bella").unwrap())
            .write(&path)
            .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "voice = \"af_bella\"\n");
    }

    #[test]
    fn set_speed_writes_shortest_float() {
        let dir = TempDir::new("config-float");
        let path = dir.path().join("config.toml");
        Setting::Speed(Speed::parse("1.1").unwrap())
            .write(&path)
            .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "speed = 1.1\n");
    }

    #[test]
    fn load_overlays_defaults_and_accepts_integer_speed() {
        let dir = TempDir::new("config-load");
        let path = dir.path().join("config.toml");
        fs::write(&path, "speed = 2\nplayer = [\"p\"]\nextra = 1\n").unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.speed.get(), 2.0);
        assert_eq!(config.player, Argv::new(vec!["p".into()]).unwrap());
        assert_eq!(config.voice.as_str(), "af_heart");
    }

    #[test]
    fn load_reads_wake_lead_and_sink_probe() {
        let dir = TempDir::new("config-wake");
        let path = dir.path().join("config.toml");
        assert_eq!(Config::defaults().wake_lead, Duration::from_millis(650));
        for ms in [0, 350, 2000] {
            fs::write(
                &path,
                format!("wake_lead_ms = {ms}\nsink_probe = [\"probe\"]\n"),
            )
            .unwrap();
            let config = Config::load(&path).unwrap();
            assert_eq!(config.wake_lead, Duration::from_millis(ms));
            assert_eq!(config.sink_probe, Argv::new(vec!["probe".into()]).unwrap());
        }
    }

    #[test]
    fn load_reads_any_lang_as_en_us() {
        let dir = TempDir::new("config-lang");
        let path = dir.path().join("config.toml");
        fs::write(&path, "lang = \"en-gb\"\n").unwrap();
        assert_eq!(
            Config::load(&path).unwrap().to_json(),
            Config::defaults().to_json()
        );
    }

    #[test]
    fn load_rejects_malformed_and_bad_values_without_panicking() {
        let dir = TempDir::new("config-bad");
        let path = dir.path().join("config.toml");
        for (body, want) in [
            ("speed = \"fast\"\n", "config.toml: speed must be a number"),
            ("lang = 1\n", "config.toml: lang must be a string"),
            (
                "speed = 3.0\n",
                "config.toml: speed must be between 0.5 and 2.0",
            ),
            (
                "player = []\n",
                "config.toml: player must be a non-empty list of strings",
            ),
            (
                "wake_lead_ms = 2001\n",
                "config.toml: wake_lead_ms must be an integer between 0 and 2000",
            ),
            (
                "wake_lead_ms = -1\n",
                "config.toml: wake_lead_ms must be an integer between 0 and 2000",
            ),
            (
                "wake_lead_ms = 600.0\n",
                "config.toml: wake_lead_ms must be an integer between 0 and 2000",
            ),
        ] {
            fs::write(&path, body).unwrap();
            assert_eq!(Config::load(&path).unwrap_err().to_string(), want);
        }
        fs::write(&path, "voice = \n").unwrap();
        assert!(Config::load(&path).is_err());
        fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn json_is_one_sorted_line_with_numeric_speed() {
        assert_eq!(
            Config::defaults().to_json(),
            r#"{"capture_clipboard":["wl-paste"],"capture_primary":["wl-paste","--primary"],"lang":"en-us","notify":["notify-send","Omatalk"],"player":["pw-cat","-p","--raw","--format","s16"],"sink_probe":["pactl"],"speed":1.0,"voice":"af_heart","wake_lead_ms":650}"#
        );
    }
}
