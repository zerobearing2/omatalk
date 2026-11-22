//! scripts/build.sh --pack-only and the systemd unit.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

use common::{TempDir, chmod, read, read_tar_gz, repo, run, sha256, write};

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
fn pack_is_byte_identical_across_runs() {
    let tmp = TempDir::new("pack");
    let tree = checkout(tmp.path());

    let first = pack(&tree);
    std::fs::File::options()
        .write(true)
        .open(tree.join("systemd/omatalk.service"))
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(1))
        .unwrap();
    let second = pack(&tree);

    assert_eq!(first, second);
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
fn unit_runs_the_packaged_binary_as_the_daemon() {
    let unit = read(repo("systemd/omatalk.service"));
    let lines: Vec<&str> = unit.lines().collect();

    for want in [
        "ExecStart=/usr/bin/omatalk daemon",
        "Environment=OMATALK_MODELS=%h/.local/share/omatalk/models",
        "Restart=always",
        "RestartSec=500ms",
        "WantedBy=graphical-session.target",
    ] {
        assert!(lines.contains(&want), "missing {want:?}");
    }
}

/// The `Key = value` lines of an alpm hook, as pairs.
fn hook(name: &str) -> Vec<(String, String)> {
    read(repo(&format!("packaging/dev/{name}")))
        .lines()
        .filter_map(|line| line.split_once(" = "))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn hook_has(hook: &[(String, String)], key: &str, value: &str) -> bool {
    hook.iter().any(|(k, v)| k == key && v == value)
}

#[test]
fn remove_hook_runs_before_removal_and_aborts_on_failure() {
    let text = read(repo("packaging/dev/remove-user-services.hook"));
    let hook = hook("remove-user-services.hook");

    assert!(hook_has(&hook, "Operation", "Remove"));
    assert!(hook_has(&hook, "Target", "omatalk-bin"));
    assert!(hook_has(&hook, "When", "PreTransaction"));
    assert!(hook_has(
        &hook,
        "Exec",
        "/usr/lib/omatalk/package-remove omatalk"
    ));
    assert!(text.lines().any(|l| l == "AbortOnFail"));
}

#[test]
fn restart_hook_runs_after_an_upgrade_and_never_fails_the_update() {
    let text = read(repo("packaging/dev/restart-user-services.hook"));
    let hook = hook("restart-user-services.hook");

    assert!(hook_has(&hook, "Operation", "Upgrade"));
    assert!(hook_has(&hook, "Target", "omatalk-bin"));
    assert!(hook_has(&hook, "When", "PostTransaction"));
    assert!(hook_has(
        &hook,
        "Exec",
        "/usr/lib/omatalk/package-restart omatalk"
    ));
    assert!(!text.contains("AbortOnFail"));
}

/// `package-remove --user APP HOME RUNTIME` for a user with no manager running.
fn package_remove_user(app: &str, tmp: &Path) -> common::Run {
    run(
        Command::new("bash")
            .arg(repo("packaging/dev/package-remove"))
            .args(["--user", app])
            .arg(tmp.join("home"))
            .arg(tmp.join("runtime")),
        "",
    )
}

#[test]
fn package_remove_accepts_omatalk_and_drops_only_its_enablement_link() {
    let tmp = TempDir::new("hook");
    let wants = tmp
        .path()
        .join("home/.config/systemd/user/graphical-session.target.wants");
    std::fs::create_dir_all(&wants).unwrap();
    for unit in ["omatalk", "other"] {
        std::os::unix::fs::symlink(
            format!("/usr/lib/systemd/user/{unit}.service"),
            wants.join(format!("{unit}.service")),
        )
        .unwrap();
    }

    let result = package_remove_user("omatalk", tmp.path());

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!wants.join("omatalk.service").is_symlink());
    assert!(wants.join("other.service").is_symlink());
}

#[test]
fn package_remove_and_restart_refuse_every_other_app() {
    let tmp = TempDir::new("hook");
    for app in ["omaspeak", "omawake", ""] {
        assert_eq!(package_remove_user(app, tmp.path()).code, 2, "{app:?}");
        for script in ["package-remove", "package-restart"] {
            let result = run(
                Command::new("bash")
                    .arg(repo(&format!("packaging/dev/{script}")))
                    .arg(app),
                "",
            );
            assert_eq!(result.code, 2, "{script} {app:?}");
        }
    }
}
