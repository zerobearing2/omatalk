//! scripts/release.sh and the Makefile's rc and release targets, in a real
//! git repo with a local bare origin, a recording `gh`, and a stub `cargo`.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::{Run, System, chmod, names, read, repo, run, write};

const MANIFEST: &str = "[dependencies.early]\nversion = \"1.0.0\"\n\n\
                        [package]\nname = \"omatalk\"\nversion = \"0.9.0\"\n\n\
                        [dependencies.late]\nversion = \"2.0.0\"\n";
const INSTALLER: &str = "#!/bin/sh\nRELEASE_TAG=\"v0.9.0\"\nTARBALL_SHA256=\"0\"\n";
const GH_LATEST: &str = "gh api repos/zerobearing2/omatalk/releases/latest --jq .tag_name";

/// Everything a refused or failed run must leave alone.
#[derive(Debug, PartialEq)]
struct Observed {
    origin_refs: String,
    head: String,
    branch: String,
    status: String,
    worktrees: String,
}

struct Fixture {
    sys: System,
    work: PathBuf,
    origin: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let sys = System::new();
        let work = sys.path("repo");
        let origin = sys.path("origin.git");
        let fx = Fixture { sys, work, origin };

        std::fs::create_dir_all(&fx.origin).unwrap();
        fx.git(&fx.origin, &["init", "--quiet", "--bare", "-b", "master"]);
        std::fs::create_dir_all(&fx.work).unwrap();
        fx.git(&fx.work, &["init", "--quiet", "-b", "master"]);
        for (key, value) in [
            ("user.name", "Release Test"),
            ("user.email", "release@test.invalid"),
            ("commit.gpgsign", "false"),
            ("tag.gpgsign", "false"),
        ] {
            fx.git(&fx.work, &["config", key, value]);
        }

        for name in [
            "scripts/release.sh",
            "scripts/build.sh",
            "scripts/lib.sh",
            "systemd/omatalk.service",
            "LICENSE",
            "Makefile",
        ] {
            write(fx.work.join(name), std::fs::read(repo(name)).unwrap());
        }
        chmod(fx.work.join("scripts/release.sh"), 0o755);
        chmod(fx.work.join("scripts/build.sh"), 0o755);
        write(fx.work.join("Cargo.toml"), MANIFEST);
        write(fx.work.join("Cargo.lock"), "# lock\n");
        write(fx.work.join("install.sh"), INSTALLER);
        write(fx.work.join("uninstall.sh"), "#!/bin/sh\n");
        write(fx.work.join(".gitignore"), "/target\n");
        write(fx.sys.path("state/latest"), "v0.9.0\n");

        fx.git(&fx.work, &["add", "-A"]);
        fx.git(&fx.work, &["commit", "--quiet", "-m", "Release v0.9.0"]);
        let origin = fx.origin.to_str().unwrap();
        fx.git(&fx.work, &["remote", "add", "origin", origin]);
        fx.git(&fx.work, &["push", "--quiet", "-u", "origin", "master"]);
        fx
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("HOME", self.sys.home(""))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn work_git(&self, args: &[&str]) -> String {
        self.git(&self.work, args)
    }

    fn origin_git(&self, args: &[&str]) -> String {
        self.git(&self.origin, args)
    }

    /// A commit on master, pushed to origin.
    fn push_commit(&self, name: &str) {
        self.push_commit_to("master", name);
    }

    /// A commit on the checked-out branch, pushed to origin's `branch`.
    fn push_commit_to(&self, branch: &str, name: &str) {
        write(self.work.join(name), name);
        self.work_git(&["add", name]);
        self.work_git(&["commit", "--quiet", "-m", name]);
        self.work_git(&["push", "--quiet", "origin", branch]);
    }

    /// A tag at the current HEAD, pushed to origin.
    fn push_tag(&self, tag: &str) {
        self.work_git(&["tag", tag]);
        self.work_git(&["push", "--quiet", "origin", &format!("refs/tags/{tag}")]);
    }

    fn observe(&self) -> Observed {
        Observed {
            origin_refs: self.origin_git(&["for-each-ref", "--format=%(objectname) %(refname)"]),
            head: self.work_git(&["rev-parse", "HEAD"]),
            branch: self.work_git(&["rev-parse", "--abbrev-ref", "HEAD"]),
            status: self.work_git(&["status", "--porcelain"]),
            worktrees: self.work_git(&["worktree", "list", "--porcelain"]),
        }
    }

    fn origin_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .origin_git(&["tag", "--list"])
            .lines()
            .map(str::to_owned)
            .collect();
        tags.sort();
        tags
    }

    fn origin_master(&self) -> String {
        self.origin_git(&["rev-parse", "refs/heads/master"])
    }

    fn script(&self, mode: &str, version: &str) -> Command {
        let mut cmd = self.sys.command("bash");
        cmd.arg(self.work.join("scripts/release.sh"))
            .arg(mode)
            .current_dir(&self.work)
            .env("VERSION", version)
            .env_remove("CARGO_TARGET_DIR");
        cmd
    }

    fn release(&self, mode: &str, version: &str) -> Run {
        run(&mut self.script(mode, version), "")
    }

    fn make(&self, target: &str, version: &str) -> Run {
        let mut cmd = self.sys.command("make");
        cmd.arg(target)
            .arg(format!("VERSION={version}"))
            .current_dir(&self.work)
            .env_remove("CARGO_TARGET_DIR");
        run(&mut cmd, "")
    }

    fn calls(&self, program: &str) -> Vec<String> {
        let prefix = format!("{program} ");
        self.sys
            .commands()
            .into_iter()
            .filter(|line| line.starts_with(&prefix))
            .collect()
    }

    fn release_creates(&self) -> Vec<String> {
        self.calls("gh")
            .into_iter()
            .filter(|line| line.starts_with("gh release create "))
            .collect()
    }

    fn latest(&self) -> String {
        read(self.sys.path("state/latest")).trim().to_owned()
    }

    fn set_latest(&self, tag: &str) {
        write(self.sys.path("state/latest"), format!("{tag}\n"));
    }

    /// A branch at the current HEAD, pushed to origin and checked out.
    fn release_branch(&self, name: &str) {
        self.work_git(&["switch", "--quiet", "-c", name]);
        self.work_git(&["push", "--quiet", "-u", "origin", name]);
    }

    fn switch(&self, name: &str) {
        self.work_git(&["switch", "--quiet", name]);
    }

    fn origin_branch(&self, name: &str) -> String {
        self.origin_git(&["rev-parse", &format!("refs/heads/{name}")])
    }

    /// The last `gh release create` of the run, as words.
    fn last_create(&self) -> Vec<String> {
        let creates = self.release_creates();
        words(creates.last().expect("no gh release create"))
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn show(&self, rev_path: &str) -> String {
        self.origin_git(&["show", rev_path])
    }

    /// The caller's checkout is as it was. Origin gains only the new tag.
    fn assert_caller_unchanged(&self, before: &Observed) {
        let mut after = self.observe();
        after.origin_refs = before.origin_refs.clone();
        assert_eq!(&after, before);
    }

    /// No temporary worktree is registered or left behind.
    fn assert_no_worktree_left(&self) {
        let list = self.work_git(&["worktree", "list", "--porcelain"]);
        assert_eq!(list.matches("worktree ").count(), 1, "{list}");
        assert_eq!(names(self.sys.path("tmp")), Vec::<String>::new());
    }
}

/// The run was refused with one stderr line, before anything changed.
fn assert_refused(fx: &Fixture, before: &Observed, result: &Run, needle: &str) {
    assert_eq!(result.code, 1, "{}", result.stderr);
    assert_eq!(result.stderr.lines().count(), 1, "{}", result.stderr);
    assert!(result.stderr.contains(needle), "{}", result.stderr);
    assert_eq!(&fx.observe(), before);
    assert_eq!(fx.calls("cargo"), Vec::<String>::new());
    assert_eq!(fx.calls("gh"), Vec::<String>::new());
    fx.assert_no_worktree_left();
}

fn words(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

#[test]
fn rc_publishes_only_a_tag_and_a_prerelease() {
    let fx = Fixture::new();
    let master = fx.origin_master();
    let before = fx.observe();

    let result = fx.release("rc", "0.10.0");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fx.origin_tags(), ["v0.10.0-rc.1"]);
    assert_eq!(fx.origin_master(), master);
    fx.assert_caller_unchanged(&before);
    fx.assert_no_worktree_left();

    let tag = "refs/tags/v0.10.0-rc.1";
    assert_eq!(fx.origin_git(&["rev-parse", &format!("{tag}^")]), master);
    assert_eq!(
        fx.origin_git(&["log", "-1", "--format=%s", tag]),
        "Release v0.10.0-rc.1"
    );
    assert_eq!(
        fx.show(&format!("{tag}:Cargo.toml")),
        MANIFEST.replace("\"0.9.0\"", "\"0.10.0-rc.1\"").trim()
    );
    let pin = fx.show(&format!("{tag}:install.sh"));
    assert!(pin.contains("RELEASE_TAG=\"v0.10.0-rc.1\""), "{pin}");
    assert!(!pin.contains("TARBALL_SHA256=\"0\""), "{pin}");
    assert!(fx.show("master:Cargo.toml").contains("version = \"0.9.0\""));

    assert_eq!(
        fx.calls("cargo"),
        [
            "cargo update --workspace --offline --quiet",
            "cargo test --locked",
            "cargo fmt --check",
            "cargo clippy --locked --all-targets -- -D warnings",
            "cargo build --release --locked",
        ]
    );
    assert!(fx.work.join("target/release/omatalk").is_file());

    let creates = fx.release_creates();
    assert_eq!(creates.len(), 1, "{creates:?}");
    let create = words(&creates[0]);
    assert_eq!(&create[..4], ["gh", "release", "create", "v0.10.0-rc.1"]);
    for want in [
        "--verify-tag",
        "--generate-notes",
        "--prerelease",
        "--latest=false",
        "omatalk-x86_64.tar.gz",
        "omatalk-x86_64.tar.gz.sha256",
        "install.sh",
        "uninstall.sh",
    ] {
        assert!(create.contains(&want), "{want} missing from {create:?}");
    }
    assert!(!create.contains(&"--latest"), "{create:?}");
    assert_eq!(
        fx.calls("gh")
            .iter()
            .filter(|line| line.as_str() == GH_LATEST)
            .count(),
        2
    );
    assert_eq!(fx.latest(), "v0.9.0");

    assert!(result.stdout.contains(
        "bash -c \"$(curl -fsSL https://github.com/zerobearing2/omatalk/releases/download/v0.10.0-rc.1/install.sh)\""
    ), "{}", result.stdout);
}

#[test]
fn rc_numbers_count_up_per_version() {
    let fx = Fixture::new();

    for want in ["v0.10.0-rc.1", "v0.10.0-rc.2", "v0.10.0-rc.3"] {
        let result = fx.release("rc", "0.10.0");
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert!(result.stdout.contains(&format!("Released {want}")));
    }
    assert_eq!(
        fx.origin_tags(),
        ["v0.10.0-rc.1", "v0.10.0-rc.2", "v0.10.0-rc.3"]
    );

    for tag in ["v0.10.0-rc.9", "v0.10.0-rc.10", "v0.11.0-rc.20"] {
        fx.push_tag(tag);
    }
    let result = fx.release("rc", "0.10.0");
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(fx.origin_tags().contains(&"v0.10.0-rc.11".to_owned()));
}

#[test]
fn make_rc_and_make_release_run_the_script() {
    let fx = Fixture::new();

    let rc = fx.make("rc", "0.10.0");
    assert_eq!(rc.code, 0, "{}", rc.stderr);
    let stable = fx.make("release", "0.10.0");
    assert_eq!(stable.code, 0, "{}", stable.stderr);

    assert_eq!(fx.origin_tags(), ["v0.10.0", "v0.10.0-rc.1"]);
}

#[test]
fn stable_publishes_latest_from_the_master_the_last_rc_tested() {
    let fx = Fixture::new();
    let master = fx.origin_master();
    assert_eq!(fx.release("rc", "0.10.0").code, 0);
    assert_eq!(fx.release("rc", "0.10.0").code, 0);
    let before = fx.observe();

    let result = fx.release("stable", "0.10.0");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        fx.origin_tags(),
        ["v0.10.0", "v0.10.0-rc.1", "v0.10.0-rc.2"]
    );
    assert_eq!(fx.origin_master(), master);
    fx.assert_caller_unchanged(&before);
    fx.assert_no_worktree_left();
    assert_eq!(fx.origin_git(&["rev-parse", "refs/tags/v0.10.0^"]), master);
    assert_eq!(
        fx.show("refs/tags/v0.10.0:Cargo.toml"),
        MANIFEST.replace("\"0.9.0\"", "\"0.10.0\"").trim()
    );
    assert!(
        fx.show("refs/tags/v0.10.0:install.sh")
            .contains("RELEASE_TAG=\"v0.10.0\"")
    );

    let creates = fx.release_creates();
    let create = words(creates.last().unwrap());
    assert_eq!(&create[..4], ["gh", "release", "create", "v0.10.0"]);
    assert!(create.contains(&"--latest"), "{create:?}");
    assert!(!create.contains(&"--prerelease"), "{create:?}");
    assert!(!create.contains(&"--latest=false"), "{create:?}");
    assert_eq!(fx.latest(), "v0.10.0");
    assert!(!result.stdout.contains("curl"), "{}", result.stdout);
}

#[test]
fn a_moved_latest_fails_the_run() {
    let fx = Fixture::new();

    let mut rc = fx.script("rc", "0.10.0");
    rc.env("FAKE_GH_LATEST_AFTER", "v0.10.0-rc.1");
    let result = run(&mut rc, "");
    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("releases/latest moved"),
        "{}",
        result.stderr
    );

    let fx = Fixture::new();
    assert_eq!(fx.release("rc", "0.10.0").code, 0);
    let mut stable = fx.script("stable", "0.10.0");
    stable.env("FAKE_GH_LATEST_AFTER", "v0.9.0");
    let result = run(&mut stable, "");
    assert_eq!(result.code, 1);
    assert!(result.stderr.contains("not v0.10.0"), "{}", result.stderr);
}

#[test]
fn refuses_a_bad_mode_and_a_bad_version() {
    let fx = Fixture::new();
    let before = fx.observe();

    let mut cmd = fx.script("", "0.10.0");
    assert_refused(&fx, &before, &run(&mut cmd, ""), "usage");
    let mut cmd = fx.script("bogus", "0.10.0");
    assert_refused(&fx, &before, &run(&mut cmd, ""), "usage");

    let mut cmd = fx.script("rc", "");
    cmd.env_remove("VERSION");
    assert_refused(&fx, &before, &run(&mut cmd, ""), "VERSION is required");

    for version in [
        "1.2",
        "v1.2.3",
        "1.2.3.4",
        "1.2.3-rc.1",
        "1.2.3-dev.1",
        "1.2.x",
    ] {
        for mode in ["rc", "stable"] {
            let result = fx.release(mode, version);
            assert_refused(&fx, &before, &result, "VERSION must be exactly x.y.z");
        }
    }
}

#[test]
fn refuses_off_master_a_dirty_tree_and_a_master_that_differs_from_origin() {
    let fx = Fixture::new();

    fx.work_git(&["switch", "--quiet", "-c", "topic"]);
    let before = fx.observe();
    for mode in ["rc", "stable"] {
        let result = fx.release(mode, "0.10.0");
        assert_refused(
            &fx,
            &before,
            &result,
            "need branch master or release-X.Y.x, on topic",
        );
    }
    fx.work_git(&["switch", "--quiet", "master"]);

    write(fx.work.join("Cargo.toml"), "edited");
    let before = fx.observe();
    for mode in ["rc", "stable"] {
        let result = fx.release(mode, "0.10.0");
        assert_refused(&fx, &before, &result, "not clean");
    }
    fx.work_git(&["checkout", "--quiet", "Cargo.toml"]);

    write(fx.work.join("stray.txt"), "untracked");
    let before = fx.observe();
    let result = fx.release("rc", "0.10.0");
    assert_refused(&fx, &before, &result, "not clean");
    std::fs::remove_file(fx.work.join("stray.txt")).unwrap();

    let other = fx.sys.path("other");
    fx.git(
        &fx.sys.path(""),
        &["clone", "--quiet", fx.origin.to_str().unwrap(), "other"],
    );
    for (key, value) in [("user.name", "Other"), ("user.email", "other@test.invalid")] {
        fx.git(&other, &["config", key, value]);
    }
    write(other.join("ahead.txt"), "ahead");
    fx.git(&other, &["add", "ahead.txt"]);
    fx.git(&other, &["commit", "--quiet", "-m", "ahead"]);
    fx.git(&other, &["push", "--quiet", "origin", "master"]);
    let before = fx.observe();
    for mode in ["rc", "stable"] {
        let result = fx.release(mode, "0.10.0");
        assert_refused(&fx, &before, &result, "not origin/master");
    }
    fx.work_git(&["pull", "--quiet", "--ff-only", "origin", "master"]);

    write(fx.work.join("unpushed.txt"), "unpushed");
    fx.work_git(&["add", "unpushed.txt"]);
    fx.work_git(&["commit", "--quiet", "-m", "unpushed"]);
    let before = fx.observe();
    let result = fx.release("rc", "0.10.0");
    assert_refused(&fx, &before, &result, "not origin/master");
}

#[test]
fn refuses_a_version_whose_tag_exists() {
    let fx = Fixture::new();
    fx.push_tag("v0.10.0");
    let before = fx.observe();

    for mode in ["rc", "stable"] {
        let result = fx.release(mode, "0.10.0");
        assert_refused(&fx, &before, &result, "v0.10.0 already exists");
    }
}

#[test]
fn refuses_stable_without_an_rc() {
    let fx = Fixture::new();
    fx.push_tag("v0.11.0-rc.1");
    let before = fx.observe();

    let result = fx.release("stable", "0.10.0");

    assert_refused(&fx, &before, &result, "no v0.10.0-rc.N tag");
}

#[test]
fn refuses_stable_when_master_moved_past_the_last_rc() {
    let fx = Fixture::new();
    assert_eq!(fx.release("rc", "0.10.0").code, 0);
    fx.push_commit("landed-after-rc.txt");
    std::fs::remove_file(fx.sys.path("commands.log")).unwrap();
    let before = fx.observe();

    let result = fx.release("stable", "0.10.0");

    assert_refused(&fx, &before, &result, "cut another RC");
    assert_eq!(fx.release("rc", "0.10.0").code, 0);
    assert_eq!(fx.release("stable", "0.10.0").code, 0);
    assert_eq!(fx.latest(), "v0.10.0");
}

#[test]
fn a_failing_step_leaves_no_tag_release_worktree_or_change() {
    for step in ["test", "fmt", "build"] {
        let fx = Fixture::new();
        let before = fx.observe();

        let mut cmd = fx.script("rc", "0.10.0");
        cmd.env("FAKE_CARGO_FAIL", step);
        let result = run(&mut cmd, "");

        assert_ne!(result.code, 0, "{step}");
        assert_eq!(fx.origin_tags(), Vec::<String>::new(), "{step}");
        assert_eq!(fx.work_git(&["tag", "--list"]), "", "{step}");
        assert_eq!(fx.release_creates(), Vec::<String>::new(), "{step}");
        assert_eq!(fx.observe(), before, "{step}");
        fx.assert_no_worktree_left();
    }
}

#[test]
fn a_rejected_tag_push_leaves_no_local_tag_and_no_release() {
    let fx = Fixture::new();
    write(
        fx.origin.join("hooks/pre-receive"),
        "#!/bin/sh\necho rejected >&2\nexit 1\n",
    );
    chmod(fx.origin.join("hooks/pre-receive"), 0o755);
    let before = fx.observe();

    let result = fx.release("rc", "0.10.0");

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("could not push v0.10.0-rc.1"),
        "{}",
        result.stderr
    );
    assert_eq!(fx.work_git(&["tag", "--list"]), "");
    assert_eq!(fx.release_creates(), Vec::<String>::new());
    assert_eq!(fx.observe(), before);
    fx.assert_no_worktree_left();
}

#[test]
fn refuses_a_branch_that_is_not_master_or_a_release_line() {
    let fx = Fixture::new();

    for name in [
        "release-0.9",
        "release-0.9.x-fix",
        "release-0.9.y",
        "release-0.9.1",
        "release-v0.9.x",
        "release-notes",
        "release-",
        "my-release-0.9.x",
        "release-0.9.x/topic",
    ] {
        fx.release_branch(name);
        let before = fx.observe();
        for mode in ["rc", "stable"] {
            let result = fx.release(mode, "0.9.1");
            assert_refused(
                &fx,
                &before,
                &result,
                &format!("need branch master or release-X.Y.x, on {name}"),
            );
        }
        fx.switch("master");
    }

    fx.work_git(&["switch", "--quiet", "--detach"]);
    let before = fx.observe();
    let result = fx.release("rc", "0.9.1");
    assert_refused(&fx, &before, &result, "on HEAD");
}

#[test]
fn refuses_a_version_outside_the_release_branch_line() {
    let fx = Fixture::new();
    fx.release_branch("release-0.9.x");
    let before = fx.observe();

    for version in ["0.10.0", "0.8.5", "1.9.1", "0.90.1"] {
        for mode in ["rc", "stable"] {
            let result = fx.release(mode, version);
            assert_refused(
                &fx,
                &before,
                &result,
                &format!("release-0.9.x releases 0.9.z only, not {version}"),
            );
        }
    }
}

#[test]
fn a_release_branch_must_equal_its_own_origin_branch() {
    let fx = Fixture::new();
    fx.release_branch("release-0.9.x");

    write(fx.work.join("unpushed.txt"), "unpushed");
    fx.work_git(&["add", "unpushed.txt"]);
    fx.work_git(&["commit", "--quiet", "-m", "unpushed"]);
    let before = fx.observe();
    for mode in ["rc", "stable"] {
        let result = fx.release(mode, "0.9.1");
        assert_refused(
            &fx,
            &before,
            &result,
            "release-0.9.x is not origin/release-0.9.x",
        );
    }

    fx.work_git(&["switch", "--quiet", "-c", "release-0.8.x"]);
    let before = fx.observe();
    let result = fx.release("rc", "0.8.1");
    assert_eq!(result.code, 1, "{}", result.stderr);
    assert!(
        result
            .stderr
            .contains("git fetch origin release-0.8.x --tags failed"),
        "{}",
        result.stderr
    );
    assert_eq!(fx.observe(), before);
    assert_eq!(fx.calls("gh"), Vec::<String>::new());

    fx.switch("release-0.9.x");
    write(fx.work.join("Cargo.toml"), "edited");
    let before = fx.observe();
    let result = fx.release("rc", "0.9.1");
    assert_refused(&fx, &before, &result, "not clean");
}

#[test]
fn a_release_branch_publishes_rcs_and_stable_as_tags_only() {
    let fx = Fixture::new();
    fx.release_branch("release-0.9.x");
    fx.switch("master");
    fx.push_commit("unreleased-on-master.txt");
    let master = fx.origin_master();
    fx.switch("release-0.9.x");
    fx.push_commit_to("release-0.9.x", "the-fix.txt");
    let branch = fx.origin_branch("release-0.9.x");
    let before = fx.observe();

    for want in ["v0.9.1-rc.1", "v0.9.1-rc.2"] {
        let result = fx.release("rc", "0.9.1");
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert!(result.stdout.contains(&format!("Released {want}")));
    }
    assert_eq!(fx.origin_tags(), ["v0.9.1-rc.1", "v0.9.1-rc.2"]);
    assert_eq!(fx.latest(), "v0.9.0");

    let result = fx.release("stable", "0.9.1");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fx.origin_tags(), ["v0.9.1", "v0.9.1-rc.1", "v0.9.1-rc.2"]);
    assert_eq!(fx.origin_master(), master);
    assert_eq!(fx.origin_branch("release-0.9.x"), branch);
    fx.assert_caller_unchanged(&before);
    fx.assert_no_worktree_left();

    let tag = "refs/tags/v0.9.1";
    assert_eq!(fx.origin_git(&["rev-parse", &format!("{tag}^")]), branch);
    let tree = fx.origin_git(&["ls-tree", "-r", "--name-only", tag]);
    assert!(tree.lines().any(|l| l == "the-fix.txt"), "{tree}");
    assert!(
        !tree.lines().any(|l| l == "unreleased-on-master.txt"),
        "{tree}"
    );
    assert!(
        fx.show(&format!("{tag}:Cargo.toml"))
            .contains("version = \"0.9.1\"")
    );
    assert!(fx.last_create().contains(&"--latest".to_owned()));
    assert_eq!(fx.latest(), "v0.9.1");
}

#[test]
fn stable_on_an_old_line_leaves_a_newer_latest_alone() {
    let fx = Fixture::new();
    fx.release_branch("release-0.9.x");
    fx.set_latest("v0.10.0");
    assert_eq!(fx.release("rc", "0.9.12").code, 0);

    let result = fx.release("stable", "0.9.12");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains(
            "v0.9.12 is not higher than latest v0.10.0; it does not take releases/latest."
        ),
        "{}",
        result.stdout
    );
    let create = fx.last_create();
    assert_eq!(&create[3], "v0.9.12");
    assert!(create.contains(&"--latest=false".to_owned()), "{create:?}");
    assert!(!create.contains(&"--latest".to_owned()), "{create:?}");
    assert!(!create.contains(&"--prerelease".to_owned()), "{create:?}");
    assert_eq!(fx.latest(), "v0.10.0");
    assert!(fx.origin_tags().contains(&"v0.9.12".to_owned()));
}

#[test]
fn stable_takes_latest_only_when_numerically_higher() {
    let fx = Fixture::new();
    fx.set_latest("v0.9.2");
    assert_eq!(fx.release("rc", "0.10.0").code, 0);
    let result = fx.release("stable", "0.10.0");
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(fx.last_create().contains(&"--latest".to_owned()));
    assert_eq!(fx.latest(), "v0.10.0");
    assert!(
        !result.stdout.contains("does not take"),
        "{}",
        result.stdout
    );

    for (latest, version) in [
        ("v0.9.2", "0.9.3"),
        ("v0.9.9", "1.0.0"),
        ("v1.2.3", "1.2.10"),
    ] {
        let fx = Fixture::new();
        fx.set_latest(latest);
        assert_eq!(fx.release("rc", version).code, 0);
        assert_eq!(fx.release("stable", version).code, 0);
        assert!(fx.last_create().contains(&"--latest".to_owned()));
        assert_eq!(fx.latest(), format!("v{version}"));
    }

    for (latest, version) in [
        ("v0.10.0", "0.9.12"),
        ("v1.0.0", "0.99.99"),
        ("v1.2.10", "1.2.9"),
    ] {
        let fx = Fixture::new();
        fx.set_latest(latest);
        assert_eq!(fx.release("rc", version).code, 0);
        let result = fx.release("stable", version);
        assert_eq!(result.code, 0, "{} {}", latest, result.stderr);
        assert!(fx.last_create().contains(&"--latest=false".to_owned()));
        assert_eq!(fx.latest(), latest);
    }
}

#[test]
fn an_unreadable_latest_refuses_before_anything_is_pushed() {
    let fx = Fixture::new();

    for mode in ["rc", "stable"] {
        if mode == "stable" {
            assert_eq!(fx.release("rc", "0.10.0").code, 0);
        }
        let before = fx.observe();
        let tags = fx.origin_tags();
        let local_tags = fx.work_git(&["tag", "--list"]);
        let creates = fx.release_creates();

        let mut cmd = fx.script(mode, "0.10.0");
        cmd.env("FAKE_GH_API_FAIL", "1");
        let result = run(&mut cmd, "");

        assert_eq!(result.code, 1, "{mode}: {}", result.stderr);
        assert_eq!(result.stderr.lines().count(), 1, "{}", result.stderr);
        assert!(
            result.stderr.contains(
                "could not read releases/latest (gh: Internal Server Error (HTTP 500)); nothing was pushed"
            ),
            "{}",
            result.stderr
        );
        assert_eq!(fx.observe(), before);
        assert_eq!(fx.origin_tags(), tags);
        assert_eq!(fx.work_git(&["tag", "--list"]), local_tags);
        assert_eq!(fx.release_creates(), creates);
        fx.assert_no_worktree_left();
    }
}

#[test]
fn a_missing_latest_release_is_not_an_error() {
    let fx = Fixture::new();
    std::fs::remove_file(fx.sys.path("state/latest")).unwrap();

    let rc = fx.release("rc", "0.10.0");
    assert_eq!(rc.code, 0, "{}", rc.stderr);
    assert!(!fx.sys.path("state/latest").exists());

    let stable = fx.release("stable", "0.10.0");
    assert_eq!(stable.code, 0, "{}", stable.stderr);
    assert!(fx.last_create().contains(&"--latest".to_owned()));
    assert_eq!(fx.latest(), "v0.10.0");
}
