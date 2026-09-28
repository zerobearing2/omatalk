//! A release site, install.sh pinned to it, and seeds of earlier installs,
//! on top of the fake machine in common::System.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::common::{Run, System, TempDir, chmod, read, repo, run, sha256, sha256_bytes, write};

pub use omatalk::speech::kokoro::MODEL_FILE as FP32;
pub use omatalk::voices::VOICES_FILE as VOICES;

pub const PLUGIN_ADD_CMD: &str =
    "omarchy plugin add https://example.test/omarchy-omatalk-plugin.git --enable --yes";
pub const TARBALL: &str = "omatalk-x86_64.tar.gz";
pub const FP16: &str = "kokoro-v1.0.fp16.onnx";
pub const SITE_URL: &str = "https://release.test";
pub const BIND_LINE: &str = r#"o.bind("F8", "Omatalk", "omatalk speak")"#;

pub fn unit() -> Vec<u8> {
    std::fs::read(repo("systemd/omatalk.service")).unwrap()
}

pub fn bytes(path: impl AsRef<Path>) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

/// Stands in for the omatalk binary: status and speak against FAKE_STATE.
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
esac
"#
    )
}

pub fn release(name: &str) -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("omatalk", launcher_script(name).into_bytes()),
        ("omatalk.service", unit()),
        ("LICENSE", b"MIT\n".to_vec()),
    ]
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

/// The value of install.sh's `name` pin.
pub fn pin(name: &str) -> Option<String> {
    installer_pins(&read(repo("install.sh")))
        .into_iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
}

/// The fake machine plus a release site and install.sh's pins pointed at it.
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
        let sys = System::new();
        let model = b"fake model";
        let voices = b"fake voices";
        write(sys.site("models").join(FP32), model);
        write(sys.site("models").join(VOICES), voices);
        let pins = vec![
            ("RELEASE_BASE", SITE_URL.to_owned()),
            ("MODEL_BASE", format!("{SITE_URL}/models")),
            ("MODEL_SHA256", sha256_bytes(model)),
            ("VOICES_SHA256", sha256_bytes(voices)),
            (
                "PLUGIN_REPO",
                "https://example.test/omarchy-omatalk-plugin.git".to_owned(),
            ),
        ];
        Fake { sys, pins }
    }

    pub fn launcher(&self) -> PathBuf {
        self.home(".local/bin/omatalk")
    }

    pub fn installed_unit(&self) -> PathBuf {
        self.home(".config/systemd/user/omatalk.service")
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

    /// The release tarball at the site: `omatalk/<name>` file entries only.
    pub fn publish(&self, files: Vec<(&str, Vec<u8>)>) {
        let staging = TempDir::new("release");
        let mut members = Vec::new();
        for (name, content) in files {
            let path = staging.path().join("omatalk").join(name);
            write(&path, content);
            chmod(&path, if name == "omatalk" { 0o755 } else { 0o644 });
            members.push(format!("omatalk/{name}"));
        }
        let status = Command::new("tar")
            .arg("-czf")
            .arg(self.site(TARBALL))
            .arg("-C")
            .arg(staging.path())
            .args(members)
            .status()
            .unwrap();
        assert!(status.success(), "tar failed");
    }

    /// install.sh with its pins rewritten for the fake site.
    fn pinned_installer(&self, tarball_sha: &str) -> PathBuf {
        let mut script = read(repo("install.sh"));
        let current = installer_pins(&script);
        let pins = self
            .pins
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .chain([("TARBALL_SHA256", tarball_sha)]);
        for (name, value) in pins {
            let (_, old) = current
                .iter()
                .find(|(k, _)| k == name)
                .unwrap_or_else(|| panic!("install.sh has no {name}= pin"));
            script = script.replacen(
                &format!("{name}=\"{old}\""),
                &format!("{name}=\"{value}\""),
                1,
            );
        }
        let path = self.path("install.sh");
        write(&path, script);
        path
    }

    pub fn bash(&self, script: &Path, answer: &str) -> Run {
        run(self.command("bash").arg(script), answer)
    }

    pub fn install(&self, answer: &str) -> Run {
        self.install_with_sha(answer, &sha256(self.site(TARBALL)))
    }

    pub fn install_with_sha(&self, answer: &str, tarball_sha: &str) -> Run {
        self.bash(&self.pinned_installer(tarball_sha), answer)
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

    pub fn seed_copy_plugin(&self) {
        write(
            self.plugin_dir().join("manifest.json"),
            "{\"id\": \"zerobearing.omatalk\"}\n",
        );
        write(self.plugin_dir().join("old.txt"), "legacy copy\n");
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
