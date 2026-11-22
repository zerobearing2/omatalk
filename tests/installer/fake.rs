use std::path::{Path, PathBuf};

use crate::common::{Run, System, read, repo, run, sha256_bytes, write, write_executable};

pub use omatalk::speech::kokoro::MODEL_FILE as FP32;
pub use omatalk::voices::VOICES_FILE as VOICES;

pub const RESCAN_CMD: &str = "omarchy-shell -q shell rescanPlugins";
pub const ENABLE_CMD: &str = "omarchy plugin enable zerobearing.omatalk";
pub const FP16: &str = "kokoro-v1.0.fp16.onnx";
pub const SITE_URL: &str = "https://release.test";
pub const BIND_LINE: &str = r#"o.bind("F8", "Omatalk", "omatalk speak")"#;

pub fn bytes(path: impl AsRef<Path>) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

pub fn launcher_script(release: &str) -> String {
    format!(
        r#"#!/bin/sh
# release {release}
case "${{1:-}}" in
  status) test -f "$FAKE_STATE/ready" ;;
  speak)
    printf '%s\n' "$*" >> "$FAKE_STATE/speech"
    exit "${{FAKE_SPEAK_STATUS:-0}}"
    ;;
  setup) printf 'setup\n' >> "$FAKE_STATE/setup-calls" ;;
esac
"#
    )
}

/// The `NAME="value"` lines of an installer script.
pub fn installer_pins(script: &str) -> Vec<(String, String)> {
    script
        .lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once("=\"")?;
            let value = rest.strip_suffix('"')?;
            let is_name = !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
            (is_name && !value.contains('"')).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect()
}

pub fn pin(name: &str) -> Option<String> {
    installer_pins(&read(repo("setup.sh")))
        .into_iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
}

pub struct Fake {
    sys: System,
    pins: Vec<(&'static str, String)>,
}

impl std::ops::Deref for Fake {
    type Target = System;
    fn deref(&self) -> &System {
        &self.sys
    }
}

impl std::ops::DerefMut for Fake {
    fn deref_mut(&mut self) -> &mut System {
        &mut self.sys
    }
}

impl Fake {
    pub fn new() -> Fake {
        let mut sys = System::new();
        write_executable(sys.path("usr/bin/omatalk"), &launcher_script("package"));
        sys.set("OMATALK_BIN", sys.path("usr/bin/omatalk"));
        let model = b"fake model";
        let voices = b"fake voices";
        write(sys.site("models").join(FP32), model);
        write(sys.site("models").join(VOICES), voices);
        let plugin_source = sys.path("usr/share/omatalk/omarchy-plugin/zerobearing.omatalk");
        write(
            plugin_source.join("manifest.json"),
            "{\"id\": \"zerobearing.omatalk\"}\n",
        );
        let pins = vec![
            ("MODEL_BASE", format!("{SITE_URL}/models")),
            ("MODEL_SHA256", sha256_bytes(model)),
            ("VOICES_SHA256", sha256_bytes(voices)),
            ("PLUGIN_SOURCE", plugin_source.display().to_string()),
        ];
        Fake { sys, pins }
    }

    pub fn launcher(&self) -> PathBuf {
        self.home(".local/bin/omatalk")
    }

    pub fn installed_unit(&self) -> PathBuf {
        self.home(".config/systemd/user/omatalk.service")
    }

    pub fn plugin_source(&self) -> PathBuf {
        self.path("usr/share/omatalk/omarchy-plugin/zerobearing.omatalk")
    }

    pub fn plugin_dir(&self) -> PathBuf {
        self.home(".config/omarchy/plugins/zerobearing.omatalk")
    }

    pub fn bindings(&self) -> PathBuf {
        self.home(".config/hypr/bindings.lua")
    }

    pub fn model(&self, file: &str) -> PathBuf {
        self.install_home().join("models").join(file)
    }

    fn pinned_setup(&self) -> PathBuf {
        let mut script = read(repo("setup.sh"));
        let current = installer_pins(&script);
        for name in [
            "MODEL_BASE",
            "MODEL_SHA256",
            "VOICES_SHA256",
            "PLUGIN_SOURCE",
        ] {
            let (_, value) = self.pins.iter().find(|(k, _)| *k == name).unwrap();
            let (_, old) = current
                .iter()
                .find(|(k, _)| k == name)
                .unwrap_or_else(|| panic!("setup.sh has no {name}= pin"));
            script = script.replacen(
                &format!("{name}=\"{old}\""),
                &format!("{name}=\"{value}\""),
                1,
            );
        }
        let path = self.path("setup.sh");
        write(&path, script);
        path
    }

    pub fn setup(&self, answer: &str) -> Run {
        self.bash(&self.pinned_setup(), answer)
    }

    pub fn bash(&self, script: &Path, answer: &str) -> Run {
        run(self.command("bash").arg(script), answer)
    }

    pub fn install(&self, answer: &str) -> Run {
        self.bash(&repo("install.sh"), answer)
    }

    pub fn uninstall(&self, answer: &str) -> Run {
        self.bash(&repo("uninstall.sh"), answer)
    }

    pub fn model_requests(&self, file: &str) -> usize {
        let suffix = format!("/models/{file}");
        self.downloads()
            .iter()
            .filter(|l| l.ends_with(&suffix))
            .count()
    }

    /// The command log's last line containing `text`.
    pub fn last_index(&self, text: &str) -> usize {
        self.commands()
            .iter()
            .rposition(|l| l.contains(text))
            .unwrap_or_else(|| panic!("no {text:?} in the command log"))
    }

    /// A PATH of only the stubs plus the coreutils the installer needs: no omarchy.
    pub fn without_omarchy(&mut self) {
        let fake_bin = self.path("bin");
        std::fs::remove_file(fake_bin.join("omarchy")).unwrap();
        std::fs::remove_file(fake_bin.join("omarchy-shell")).unwrap();
        let tools = [
            "tar",
            "sha256sum",
            "mkdir",
            "rm",
            "cp",
            "mv",
            "install",
            "mktemp",
            "grep",
            "cat",
            "chmod",
            "date",
            "bash",
            "touch",
            "seq",
            "ln",
            "readlink",
            "dirname",
        ];
        for tool in tools {
            let found = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|dir| dir.join(tool))
                .find(|p| p.is_file());
            if let Some(src) = found {
                std::os::unix::fs::symlink(src, fake_bin.join(tool)).unwrap();
            }
        }
        self.set("PATH", fake_bin);
    }

    pub fn seed_checkout_plugin(&self) {
        std::fs::create_dir_all(self.plugin_dir().join(".git")).unwrap();
        write(
            self.plugin_dir().join("manifest.json"),
            "{\"id\": \"zerobearing.omatalk\"}\n",
        );
        write(self.plugin_dir().join("old.txt"), "curl-era checkout\n");
    }

    pub fn plugin_enabled(&self) -> bool {
        self.path("state/plugin-enabled").exists()
    }

    pub fn seed_curl_install(&self) -> (PathBuf, PathBuf) {
        write(self.launcher(), launcher_script("curl-era"));
        write(
            self.installed_unit(),
            "[Service]\nExecStart=%h/.local/bin/omatalk daemon\n",
        );
        write(self.model(FP32), "fake model");
        let config = self.home(".config/omatalk/config.toml");
        write(&config, "voice = \"bf_emma\"\nspeed = 1.25\n");
        write(self.bindings(), format!("-- keys\n{BIND_LINE}\n"));
        (config, self.bindings())
    }

    /// A v0.5.1 install: venv, source, fp16 model, launcher copied from the venv.
    pub fn seed_python_install(&self) -> (PathBuf, PathBuf) {
        let install_home = self.install_home();
        write(
            install_home.join("venv/bin/omatalk"),
            "#!/venv/bin/python\n",
        );
        write(install_home.join("src/daemon/omatalkd.py"), "old daemon\n");
        write(self.model(FP16), "fp16 model");
        write(self.model(VOICES), "fake voices");
        write(self.launcher(), "#!/venv/bin/python\n");
        write(self.home(".local/bin/omatalkd"), "#!/venv/bin/python\n");
        write(
            self.installed_unit(),
            "[Service]\nExecStart=%h/.local/share/omatalk/venv/bin/omatalkd\n",
        );
        let config = self.home(".config/omatalk/config.toml");
        write(
            &config,
            "# mine\nvoice = \"bf_emma\"\nlang = \"en-gb\"\nspeed = 1\n",
        );
        write(
            self.bindings(),
            "-- keys\no.bind(\"F8\", \"Omatalk\", \"omatalk speak\")\n",
        );
        (config, self.bindings())
    }

    pub fn seed_bindings(&self) -> PathBuf {
        write(
            self.bindings(),
            "-- Omatalk: F8 speaks selection\n\
             o.bind(\"F9\", \"Dictate\", \"voxtype\")\n\
             o.bind(\"F7\", \"Omatalk\", \"omatalk speak\")\n",
        );
        self.bindings()
    }
}
