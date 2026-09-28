//! A fake machine for the shell scripts: the stubs in tests/fakes/system
//! first on PATH, a HOME, a download site, and a log of every stubbed
//! command, all under one temp dir.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::Command;

use super::{TempDir, repo};

pub struct System {
    tmp: TempDir,
    env: Vec<(String, OsString)>,
}

impl System {
    pub fn new() -> System {
        let tmp = TempDir::new("system");
        let dir = tmp.path();
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        for stub in std::fs::read_dir(repo("tests/fakes/system")).unwrap() {
            let stub = stub.unwrap();
            std::os::unix::fs::symlink(stub.path(), bin.join(stub.file_name())).unwrap();
        }
        for sub in ["state", "home", "site", "tmp"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        let path = std::env::join_paths(
            std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        let env = [
            ("PATH", path),
            ("HOME", dir.join("home").into()),
            ("TMPDIR", dir.join("tmp").into()),
            ("XDG_RUNTIME_DIR", dir.join("runtime").into()),
            ("OMATALK_HOME", dir.join("omatalk").into()),
            ("OMATALK_SOCKET", dir.join("missing.sock").into()),
            ("ASK_FROM", "/dev/stdin".into()),
            ("FAKE_LOG", dir.join("commands.log").into()),
            ("FAKE_STATE", dir.join("state").into()),
            ("FAKE_SITE", dir.join("site").into()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        System { tmp, env }
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }

    pub fn home(&self, rel: &str) -> PathBuf {
        self.path("home").join(rel)
    }

    /// Where `https://<any host>/<rel>` downloads from.
    pub fn site(&self, rel: &str) -> PathBuf {
        self.path("site").join(rel)
    }

    pub fn install_home(&self) -> PathBuf {
        self.path("omatalk")
    }

    /// Applied after the inherited environment; a later value wins.
    pub fn set(&mut self, name: &str, value: impl AsRef<OsStr>) {
        self.env.push((name.to_owned(), value.as_ref().to_owned()));
    }

    pub fn env(&self) -> impl Iterator<Item = (&str, &OsStr)> {
        self.env.iter().map(|(k, v)| (k.as_str(), v.as_os_str()))
    }

    /// `program` in this machine's environment, from the repo root.
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(repo("")).envs(self.env());
        cmd
    }

    /// Every stubbed command run so far, one `name args` line each.
    pub fn commands(&self) -> Vec<String> {
        std::fs::read_to_string(self.path("commands.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub fn logged(&self, text: &str) -> bool {
        self.commands().iter().any(|l| l.contains(text))
    }

    pub fn downloads(&self) -> Vec<String> {
        self.commands()
            .into_iter()
            .filter(|l| l.starts_with("curl "))
            .collect()
    }
}
