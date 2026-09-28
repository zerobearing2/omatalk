//! scripts/build.sh --pack-only, scripts/bump.sh, and the systemd unit.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

use common::{
    System, TempDir, chmod, read, read_tar_gz, repo, run, sha256, write, write_executable,
};

const TARBALL: &str = "omatalk-x86_64.tar.gz";
const FAKE_BINARY: &[u8] = b"\x7fELF fake omatalk binary";
/// A dependency table's column-0 `version` on either side of the package's.
const MANIFEST: &str = "[dependencies.early]\nversion = \"1.0.0\"\n\n\
                        [package]\nname = \"omatalk\"\nversion = \"9.8.7\"\n\n\
                        [dependencies.late]\nversion = \"2.0.0\"\n";

/// The files build.sh reads, copied so packing never touches this tree.
fn checkout(tmp: &Path) -> PathBuf {
    let tree = tmp.join("tree");
    for name in [
        "scripts/build.sh",
        "scripts/bump.sh",
        "scripts/lib.sh",
        "systemd/omatalk.service",
        "LICENSE",
    ] {
        let dest = tree.join(name);
        write(&dest, std::fs::read(repo(name)).unwrap());
        let modified = std::fs::metadata(repo(name)).unwrap().modified().unwrap();
        std::fs::File::options()
            .write(true)
            .open(&dest)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }
    write(tree.join("Cargo.toml"), MANIFEST);
    write(
        tree.join("install.sh"),
        "RELEASE_TAG=\"v0\"\nTARBALL_SHA256=\"0\"\n",
    );
    let binary = tmp.join("target/release/omatalk");
    write(&binary, FAKE_BINARY);
    chmod(&binary, 0o700);
    tree
}

fn pack(tree: &Path) -> String {
    let result = run(
        Command::new("bash")
            .arg(tree.join("scripts/build.sh"))
            .arg("--pack-only")
            .current_dir(tree)
            .env("CARGO_TARGET_DIR", tree.parent().unwrap().join("target")),
        "",
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    let digest = sha256(tree.join(TARBALL));
    assert_eq!(result.stdout, format!("packed v9.8.7 {digest}\n"));
    assert_eq!(
        read(tree.join(format!("{TARBALL}.sha256"))),
        format!("{digest}  {TARBALL}\n")
    );
    digest
}

#[test]
fn pack_is_byte_identical_across_runs_and_leaves_install_sh() {
    let tmp = TempDir::new("pack");
    let tree = checkout(tmp.path());
    let before = read(tree.join("install.sh"));

    let first = pack(&tree);
    std::fs::File::options()
        .write(true)
        .open(tree.join("systemd/omatalk.service"))
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(1))
        .unwrap();
    let second = pack(&tree);

    assert_eq!(first, second);
    assert_eq!(read(tree.join("install.sh")), before);
}

#[test]
fn tarball_holds_the_binary_the_unit_and_the_license() {
    let tmp = TempDir::new("pack");
    let tree = checkout(tmp.path());
    pack(&tree);

    let members = read_tar_gz(tree.join(TARBALL));
    let mut names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "omatalk/LICENSE",
            "omatalk/omatalk",
            "omatalk/omatalk.service"
        ]
    );
    for member in &members {
        assert!(
            member.kind == b'0' || member.kind == 0,
            "{} is not a file",
            member.name
        );
        assert_eq!(
            (member.uid, member.gid, member.mtime),
            (0, 0, 0),
            "{}",
            member.name
        );
    }
    let member = |name: &str| members.iter().find(|m| m.name == name).unwrap();
    assert_eq!(member("omatalk/omatalk").mode, 0o755);
    assert_eq!(member("omatalk/omatalk.service").mode, 0o644);
    assert_eq!(member("omatalk/omatalk").data, FAKE_BINARY);
    assert_eq!(
        member("omatalk/omatalk.service").data,
        std::fs::read(repo("systemd/omatalk.service")).unwrap()
    );
}

#[test]
fn bump_rewrites_only_the_package_version() {
    let tmp = TempDir::new("pack");
    let tree = checkout(tmp.path());
    let sys = System::new();
    write_executable(
        sys.path("bin/cargo"),
        "#!/bin/sh\nprintf 'cargo %s\\n' \"$*\" >> \"$FAKE_LOG\"\n",
    );

    let result = run(sys.command("bash").arg(tree.join("scripts/bump.sh")), "");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        read(tree.join("Cargo.toml")),
        MANIFEST.replace("\"9.8.7\"", "\"9.8.8\"")
    );
    assert_eq!(
        sys.commands(),
        ["cargo update --workspace --offline --quiet"]
    );
}

#[test]
fn unit_runs_the_launcher_as_the_daemon() {
    let unit = read(repo("systemd/omatalk.service"));
    let lines: Vec<&str> = unit.lines().collect();

    for want in [
        "ExecStart=%h/.local/bin/omatalk daemon",
        "Environment=OMATALK_MODELS=%h/.local/share/omatalk/models",
        "Restart=always",
        "RestartSec=500ms",
        "WantedBy=graphical-session.target",
    ] {
        assert!(lines.contains(&want), "missing {want:?}");
    }
}
