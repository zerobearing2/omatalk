//! Helpers shared by the black-box tests: they drive the built binary and
//! the repo's shell scripts as separate processes.
#![allow(dead_code)]

mod system;
#[path = "../../src/testutil.rs"]
mod testutil;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[allow(unused_imports)]
pub use system::System;
#[allow(unused_imports)]
pub use testutil::{FAKES, TempDir, fake_commands, fake_env, log_lines, wait_for};

pub const OMATALK: &str = env!("CARGO_BIN_EXE_omatalk");

/// A path in the repo checkout.
pub fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

pub fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

pub fn write(path: impl AsRef<Path>, body: impl AsRef<[u8]>) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

/// An executable written by a child shell: a write fd held by this process
/// would leak into a sibling test's fork and fail its exec with ETXTBSY.
pub fn write_executable(path: impl AsRef<Path>, body: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut sh = Command::new("sh")
        .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    sh.stdin.take().unwrap().write_all(body.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "write {}", path.display());
}

pub fn chmod(path: impl AsRef<Path>, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// Sorted entry names of a directory.
pub fn names(dir: impl AsRef<Path>) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A finished process. `code` is -1 when a signal ended it.
#[derive(Debug)]
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `cmd` to completion with `stdin` as its whole input.
pub fn run(cmd: &mut Command, stdin: &str) -> Run {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {cmd:?}: {e}"));
    let mut pipe = child.stdin.take().unwrap();
    let input = stdin.to_owned();
    // A child that never reads its stdin closes the pipe; that is not an error here.
    let writer = std::thread::spawn(move || {
        let _ = pipe.write_all(input.as_bytes());
    });
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap();
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// `omatalk <args>` from the repo root.
pub fn omatalk<I, S>(args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut cmd = Command::new(OMATALK);
    cmd.args(args).current_dir(repo(""));
    cmd
}

/// Lowercase hex SHA-256 of a file, from coreutils like the installer uses.
pub fn sha256(path: impl AsRef<Path>) -> String {
    let out = Command::new("sha256sum")
        .arg(path.as_ref())
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()[..64].to_owned()
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let dir = TempDir::new("sha");
    let path = dir.path().join("blob");
    write(&path, bytes);
    sha256(path)
}

/// One member of a tar archive.
#[derive(Debug)]
pub struct TarMember {
    pub name: String,
    pub kind: u8,
    pub mode: u32,
    pub uid: u64,
    pub gid: u64,
    pub mtime: u64,
    pub data: Vec<u8>,
}

/// Members of a gzipped tar, decompressed by gzip(1) and read header by header.
pub fn read_tar_gz(path: impl AsRef<Path>) -> Vec<TarMember> {
    let out = Command::new("gzip")
        .arg("-dc")
        .arg(path.as_ref())
        .output()
        .unwrap();
    assert!(out.status.success(), "gzip -dc failed");
    let tar = out.stdout;
    let octal = |field: &[u8]| -> u64 {
        let text = String::from_utf8_lossy(field);
        let digits = text.trim_matches(|c: char| c == '\0' || c == ' ');
        u64::from_str_radix(digits, 8).unwrap_or(0)
    };
    let mut members = Vec::new();
    let mut at = 0;
    while at + 512 <= tar.len() && tar[at..at + 512].iter().any(|&b| b != 0) {
        let header = &tar[at..at + 512];
        let name_end = header[..100].iter().position(|&b| b == 0).unwrap_or(100);
        let size = octal(&header[124..136]) as usize;
        let body = at + 512;
        members.push(TarMember {
            name: String::from_utf8_lossy(&header[..name_end]).into_owned(),
            kind: header[156],
            mode: octal(&header[100..108]) as u32,
            uid: octal(&header[108..116]),
            gid: octal(&header[116..124]),
            mtime: octal(&header[136..148]),
            data: tar[body..body + size].to_vec(),
        });
        at = body + size.div_ceil(512) * 512;
    }
    members
}
