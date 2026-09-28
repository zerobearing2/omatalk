mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::{FAKES, System, TempDir, omatalk, read, run, write};

const FAKE_VOICES: [&str; 4] = ["af_heart", "af_bella", "am_test", "bf_other"];

const SITE_BASE: &str = "https://omatalk.test";

/// A fake machine whose site serves an install.sh that records
/// `$UPGRADE_VALUE` into `$UPGRADE_MARKER`.
fn upgrade_site() -> System {
    let mut sys = System::new();
    write(
        sys.site("install.sh"),
        "#!/bin/sh\nprintf '%s' \"$UPGRADE_VALUE\" > \"$UPGRADE_MARKER\"\n",
    );
    sys.set("SITE_BASE", SITE_BASE);
    sys.set("UPGRADE_MARKER", sys.path("upgrade-marker"));
    sys
}

#[test]
fn upgrade_fetches_installer_over_https_without_connecting_to_daemon() {
    let mut sys = upgrade_site();
    sys.set("UPGRADE_VALUE", "inherited");

    let result = run(omatalk(["upgrade"]).envs(sys.env()), "");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(read(sys.path("upgrade-marker")), "inherited");
    let downloads = sys.downloads();
    assert_eq!(downloads.len(), 1, "{downloads:?}");
    for flag in ["--proto =https", "--proto-redir =https", "--tlsv1.2"] {
        assert!(
            downloads[0].contains(flag),
            "{flag:?} missing: {downloads:?}"
        );
    }
    let url = downloads[0].rsplit_once(' ').unwrap().1;
    assert!(
        url.starts_with(&format!("{SITE_BASE}/install.sh?ts=")),
        "{downloads:?}"
    );
}

#[test]
fn upgrade_rejects_extra_arguments() {
    let sys = upgrade_site();

    let result = run(omatalk(["upgrade", "now"]).envs(sys.env()), "");

    assert_eq!(result.code, 2);
    assert!(
        result.stderr.starts_with("usage: omatalk"),
        "{}",
        result.stderr
    );
    assert_eq!(sys.commands(), Vec::<String>::new());
}

/// A fake machine with an installed launcher and models dir.
fn installed() -> System {
    let sys = System::new();
    write(sys.home(".local/bin/omatalk"), "launcher");
    std::fs::create_dir_all(sys.install_home().join("models")).unwrap();
    sys
}

#[test]
fn uninstall_runs_the_embedded_uninstaller() {
    let sys = installed();

    let result = run(omatalk(["uninstall"]).envs(sys.env()), "y\n");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(sys.logged("systemctl --user disable --now omatalk.service"));
    assert!(!sys.home(".local/bin/omatalk").exists());
    assert!(!sys.install_home().exists());
    assert_eq!(common::names(sys.path("tmp")), Vec::<String>::new());
    assert!(
        result.stdout.contains("Omatalk uninstalled."),
        "{}",
        result.stdout
    );
}

#[test]
fn uninstall_rejects_extra_arguments() {
    let sys = installed();

    let result = run(omatalk(["uninstall", "now"]).envs(sys.env()), "");

    assert_eq!(result.code, 2);
    assert!(
        result.stderr.starts_with("usage: omatalk"),
        "{}",
        result.stderr
    );
    assert_eq!(sys.commands(), Vec::<String>::new());
    assert!(sys.install_home().exists());
}

/// A config whose notify command is the fake, and its notify log.
fn notify_environment(tmp: &Path) -> (Vec<(&'static str, PathBuf)>, PathBuf) {
    let config = tmp.join("config.toml");
    write(&config, format!("notify = [\"{FAKES}/notify\"]\n"));
    let notify_log = tmp.join("notify.log");
    let env = vec![
        ("OMATALK_CONFIG", config),
        ("OMATALK_SOCKET", tmp.join("missing.sock")),
        ("OMATALK_TEST_NOTIFY_LOG", notify_log.clone()),
    ];
    (env, notify_log)
}

#[test]
fn version_prints_the_cargo_version_without_a_daemon() {
    for flag in ["version", "--version"] {
        let tmp = TempDir::new("cli");

        let result = run(
            omatalk([flag]).env("OMATALK_SOCKET", tmp.path().join("missing.sock")),
            "",
        );

        assert_eq!(result.code, 0, "{flag}: {}", result.stderr);
        assert_eq!(result.stdout.trim(), env!("CARGO_PKG_VERSION"), "{flag}");
        assert_eq!(result.stderr, "", "{flag}");
    }
}

#[test]
fn version_does_not_notify_when_the_daemon_is_down() {
    for flag in ["version", "--version"] {
        let tmp = TempDir::new("cli");
        let (env, notify_log) = notify_environment(tmp.path());

        let result = run(omatalk([flag]).envs(env), "");

        assert_eq!(result.code, 0, "{flag}: {}", result.stderr);
        assert!(!notify_log.exists(), "{flag}");
    }
}

#[test]
fn status_failure_does_not_notify() {
    let tmp = TempDir::new("cli");
    let (env, notify_log) = notify_environment(tmp.path());

    let result = run(omatalk(["status"]).envs(env), "");

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("daemon not running"),
        "{}",
        result.stderr
    );
    assert!(!notify_log.exists());
}

#[test]
fn speak_failure_notifies() {
    let tmp = TempDir::new("cli");
    let (env, notify_log) = notify_environment(tmp.path());

    let result = run(omatalk(["speak"]).envs(env), "");

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("daemon not running"),
        "{}",
        result.stderr
    );
    assert!(read(&notify_log).contains("daemon not running"));
}

/// A stored zip holding one empty `<name>.npy` entry per voice: the voice
/// list reads only the archive's central directory.
fn write_voices_archive(path: &Path, names: &[&str]) {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for name in names {
        let file = format!("{name}.npy");
        let offset = out.len() as u32;
        // Version 2.0, no flags, stored, 1980-01-01, CRC and sizes of empty data.
        let common = |buf: &mut Vec<u8>| {
            for half in [20u16, 0, 0, 0, 0x21] {
                buf.extend(half.to_le_bytes());
            }
            buf.extend([0u8; 12]);
            buf.extend((file.len() as u16).to_le_bytes());
            buf.extend(0u16.to_le_bytes());
        };
        out.extend(0x0403_4b50u32.to_le_bytes());
        common(&mut out);
        out.extend(file.as_bytes());
        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend(20u16.to_le_bytes());
        common(&mut central);
        central.extend([0u8; 10]);
        central.extend(offset.to_le_bytes());
        central.extend(file.as_bytes());
    }
    let central_at = out.len() as u32;
    let count = names.len() as u16;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(central_at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    write(path, out);
}

struct ConfigEnv {
    tmp: TempDir,
    config: PathBuf,
    env: Vec<(&'static str, PathBuf)>,
}

fn config_environment() -> ConfigEnv {
    let tmp = TempDir::new("cli");
    let models = tmp.path().join("models");
    write_voices_archive(&models.join("voices-v1.0.bin"), &FAKE_VOICES);
    let config = tmp.path().join("config.toml");
    let env = vec![
        ("OMATALK_CONFIG", config.clone()),
        ("OMATALK_MODELS", models),
        ("OMATALK_SOCKET", tmp.path().join("missing.sock")),
    ];
    ConfigEnv { tmp, config, env }
}

impl ConfigEnv {
    fn run(&self, args: &[&str]) -> common::Run {
        run(omatalk(args).envs(self.env.clone()), "")
    }

    fn config(&self, args: &[&str]) -> common::Run {
        run(omatalk(["config"]).args(args).envs(self.env.clone()), "")
    }

    fn written(&self) -> toml_edit::DocumentMut {
        read(&self.config).parse().unwrap()
    }
}

fn sorted_voices() -> Vec<&'static str> {
    let mut voices = FAKE_VOICES.to_vec();
    voices.sort();
    voices
}

#[test]
fn config_voices_lists_full_set_without_a_daemon() {
    let env = config_environment();

    let result = env.config(&["voices", "--json"]);

    assert_eq!(result.code, 0, "{}", result.stderr);
    let listed: Vec<String> = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(listed, sorted_voices());
}

#[test]
fn config_voices_plain_lists_one_per_line() {
    let env = config_environment();

    let result = env.config(&["voices"]);

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(result.stdout.lines().collect::<Vec<_>>(), sorted_voices());
}

#[test]
fn a_missing_voices_archive_is_reported_not_called_an_unknown_voice() {
    let env = config_environment();
    std::fs::remove_file(env.tmp.path().join("models/voices-v1.0.bin")).unwrap();

    let listed = env.config(&["voices"]);
    let set = env.config(&["set", "voice", "af_bella"]);
    let spoken = env.run(&["speak", "--voice", "af_bella", "hi"]);

    assert_eq!(listed.code, 1);
    assert!(
        listed.stderr.starts_with("voices-v1.0.bin: "),
        "{}",
        listed.stderr
    );
    for result in [set, spoken] {
        assert_eq!(result.code, 1);
        assert_eq!(result.stderr, listed.stderr);
    }
    assert!(!env.config.exists());
}

#[test]
fn config_get_json_reports_full_effective_config() {
    let env = config_environment();

    let result = env.config(&["get", "--json"]);

    assert_eq!(result.code, 0, "{}", result.stderr);
    let cfg: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(cfg["voice"], "af_heart");
    assert_eq!(cfg["speed"].as_f64(), Some(1.0));
    assert_eq!(
        cfg["player"],
        serde_json::json!(["pw-cat", "-p", "--raw", "--format", "s16"])
    );
    assert_eq!(cfg["sink_probe"], serde_json::json!(["pactl"]));
    assert_eq!(cfg["wake_lead_ms"], 650);
}

#[test]
fn readme_config_table_lists_every_key_with_its_default() {
    let env = config_environment();
    let result = env.config(&["get", "--json"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let cfg: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&result.stdout).unwrap();
    let readme = read(common::repo("README.md"));
    let rows: BTreeMap<&str, &str> = readme
        .lines()
        .skip_while(|line| !line.starts_with("| Key | Default |"))
        .skip(2)
        .take_while(|line| line.starts_with('|'))
        .map(|row| {
            let mut cells = row.split('|').skip(1).map(|c| c.trim().trim_matches('`'));
            (cells.next().unwrap(), cells.next().unwrap())
        })
        .collect();
    assert_eq!(
        rows.keys().collect::<Vec<_>>(),
        cfg.keys().collect::<Vec<_>>(),
        "README Config table keys"
    );
    let squash = |s: &str| s.split_whitespace().collect::<String>();
    for (key, default) in cfg {
        let default = match default.as_f64() {
            Some(n) if default.is_f64() => format!("{n:.1}"),
            _ => default.to_string(),
        };
        assert_eq!(
            squash(rows[key.as_str()]),
            default,
            "README default for {key}"
        );
    }
}

#[test]
fn config_get_plain_reports_key_value_lines() {
    let env = config_environment();

    let result = env.config(&["get"]);

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.lines().any(|l| l == "voice = af_heart"),
        "{}",
        result.stdout
    );
    assert!(
        result.stdout.lines().any(|l| l == "wake_lead_ms = 650"),
        "{}",
        result.stdout
    );
}

#[test]
fn config_set_voice_accepts_valid_voice_and_persists() {
    let env = config_environment();

    let result = env.config(&["set", "voice", "af_bella"]);

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(env.written()["voice"].as_str(), Some("af_bella"));
}

#[test]
fn config_set_voice_rejects_unknown_voice() {
    let env = config_environment();

    let result = env.config(&["set", "voice", "not_a_real_voice"]);

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("not_a_real_voice"),
        "{}",
        result.stderr
    );
    assert!(!env.config.exists());
}

#[test]
fn config_set_speed_accepts_valid_value() {
    let env = config_environment();

    let result = env.config(&["set", "speed", "1.5"]);

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(env.written()["speed"].as_float(), Some(1.5));
}

#[test]
fn config_set_speed_rejects_out_of_range() {
    for value in ["0.4", "2.1"] {
        let env = config_environment();

        let result = env.config(&["set", "speed", value]);

        assert_eq!(result.code, 1, "{value}");
        assert!(
            result.stderr.contains("0.5") && result.stderr.contains("2.0"),
            "{value}: {}",
            result.stderr
        );
    }
}

#[test]
fn config_set_speed_rejects_non_numeric() {
    let env = config_environment();

    let result = env.config(&["set", "speed", "fast"]);

    assert_eq!(result.code, 1);
    assert!(result.stderr.contains("fast"), "{}", result.stderr);
}

#[test]
fn config_set_rejects_unsettable_key() {
    for key in ["player", "notify", "capture_primary", "lang", "bogus"] {
        let env = config_environment();

        let result = env.config(&["set", key, "whatever"]);

        assert_eq!(result.code, 1, "{key}");
        assert!(
            result.stderr.contains("not settable via config set"),
            "{key}: {}",
            result.stderr
        );
    }
}

#[test]
fn speak_voice_rejects_unknown_voice_before_touching_daemon() {
    let env = config_environment();

    let result = env.run(&["speak", "--voice", "not_a_real_voice", "hi"]);

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("not_a_real_voice"),
        "{}",
        result.stderr
    );
    // An invalid --voice is rejected before the socket is touched at all,
    // same as `config set voice <invalid>`.
    assert!(
        !result.stderr.contains("daemon not running"),
        "{}",
        result.stderr
    );
}

#[test]
fn speak_voice_valid_reaches_the_same_daemon_down_failure_as_plain_speak() {
    let env = config_environment();
    write(&env.config, format!("notify = [\"{FAKES}/notify\"]\n"));
    let notify_log = env.tmp.path().join("notify.log");

    let result = run(
        omatalk(["speak", "--voice", "af_bella", "hi"])
            .envs(env.env.clone())
            .env("OMATALK_TEST_NOTIFY_LOG", &notify_log),
        "",
    );

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("daemon not running"),
        "{}",
        result.stderr
    );
    assert!(read(&notify_log).contains("daemon not running"));
}

#[test]
fn speak_daemon_down_survives_missing_notify_binary() {
    let tmp = TempDir::new("cli");
    let config = tmp.path().join("config.toml");
    write(&config, "notify = [\"/no-such-omatalk-notify\"]\n");

    let result = run(
        omatalk(["speak"])
            .env("OMATALK_CONFIG", &config)
            .env("OMATALK_SOCKET", tmp.path().join("missing.sock")),
        "",
    );

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("daemon not running"),
        "{}",
        result.stderr
    );
}

#[test]
fn config_set_round_trip_preserves_untouched_keys() {
    let env = config_environment();
    write(&env.config, "player = [\"custom-player\", \"--flag\"]\n");

    assert_eq!(env.config(&["set", "voice", "af_bella"]).code, 0);
    assert_eq!(env.config(&["set", "speed", "1.5"]).code, 0);

    let result = env.config(&["get", "--json"]);
    let cfg: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(cfg["voice"], "af_bella");
    assert_eq!(cfg["speed"].as_f64(), Some(1.5));
    assert_eq!(
        cfg["player"],
        serde_json::json!(["custom-player", "--flag"])
    );
}

#[test]
fn config_set_keeps_the_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let env = config_environment();
    write(&env.config, "voice = \"af_heart\"\n");
    common::chmod(&env.config, 0o600);

    let result = env.config(&["set", "speed", "1.5"]);

    assert_eq!(result.code, 0, "{}", result.stderr);
    let mode = std::fs::metadata(&env.config).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(env.written()["speed"].as_float(), Some(1.5));
}

#[test]
fn config_set_writes_through_a_dotfiles_symlink() {
    for live in [true, false] {
        let env = config_environment();
        let target = env.tmp.path().join("dotfiles/omatalk/config.toml");
        if live {
            write(&target, "voice = \"af_heart\"\n");
        }
        std::os::unix::fs::symlink("dotfiles/omatalk/config.toml", &env.config).unwrap();

        let result = env.config(&["set", "speed", "1.5"]);

        assert_eq!(result.code, 0, "live={live}: {}", result.stderr);
        let link = std::fs::symlink_metadata(&env.config).unwrap();
        assert!(link.file_type().is_symlink(), "live={live}");
        let doc: toml_edit::DocumentMut = read(&target).parse().unwrap();
        assert_eq!(doc["speed"].as_float(), Some(1.5), "live={live}");
        assert_eq!(
            doc.get("voice").and_then(|v| v.as_str()),
            live.then_some("af_heart"),
            "live={live}"
        );
    }
}
