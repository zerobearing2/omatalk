# Omatalk

Local text-to-speech for Omarchy: hotkey → the machine speaks your selected text.
See `CONTEXT.md` for domain language.

This repository is the Daemon, CLI, and site. They are one Rust crate
(`src/`) and one binary: `omatalk daemon` is the Daemon, and every other
argv is the CLI. The listed bar plugin is
https://github.com/zerobearing2/omarchy-omatalk-plugin, a separate repo with
its own release. Work on it in a plain clone of that repo
(`~/Work/omarchy-omatalk-plugin`). The installed plugin at
`~/.config/omarchy/plugins/zerobearing.omatalk` is a git clone too. Its
agent docs stay here, because the listed tree must not track `AGENTS.md`,
`CONTEXT.md`, or `docs/agents/`.

## Release

`master` is the dev line. Pushing to it publishes nothing: no release, no
site deploy, no change for stable users or the Omarchy plugin directory.
Releases are tags from master (`docs/adr/0007-releases-are-tags-from-master.md`).

```sh
make rc VERSION=0.10.0        # v0.10.0-rc.N, a GitHub prerelease
make release VERSION=0.10.0   # v0.10.0, latest; needs an RC of this exact HEAD
```

Both run `scripts/release.sh` and need `VERSION` as `x.y.z`, branch `master`
or `release-X.Y.x`, a clean tree, and HEAD equal to that branch's
`origin/<branch>`. On a release branch `VERSION` must be `X.Y.z`. Any other
branch is refused. The script builds in a temporary worktree: it sets the version, runs `cargo test --locked`,
`make lint`, and `scripts/build.sh`, commits `Release vTAG`, tags it, and
pushes only the tag. Then it runs `gh release create --verify-tag` and checks
`releases/latest`: unchanged after an RC, the new tag after a stable release.
A release never writes to master or any branch. Master's `Cargo.toml` version
is always `0.0.0-dev`, so a local build never claims to be a release; do not
change it. Master's `install.sh` pin does not track releases either. Each
tag's commit holds the real values. `make release` refuses unless the highest `v$VERSION-rc.*` tag was built
from the current HEAD (`tag^` equals HEAD), so the real release is the code
last tested. If the branch moved, cut another RC.

`releases/latest` is decided by the script, never by hand. A stable release
takes it only when its version is numerically higher than the current latest
(0.10.0 is above 0.9.2). Otherwise the script publishes with `--latest=false`,
says so, and checks that latest did not move. If it cannot read
`releases/latest` (anything but a 404) it refuses before pushing anything.

An RC installs with
`bash -c "$(curl -fsSL https://github.com/zerobearing2/omatalk/releases/download/TAG/install.sh)"`.
`releases/latest`, the site installer, and `omatalk upgrade` keep serving the
last stable release.

GitHub Actions runs `make lint` and the tests on pushes to master and on pull
requests. It never builds or publishes a release. The site (`.github/workflows/pages.yml`)
deploys when a non-prerelease is published, or on `gh workflow run pages.yml`.
The workflow deploys only when the published release is the current
`releases/latest`, so a hotfix of an old line leaves the site alone. A
site-only fix on master is not live until that dispatch, which deploys
everything in `public/` on master.

**Hotfix an old version.** Fix a shipped version from a `release-X.Y.x`
branch so unreleased master code stays out. Create it once from the version's
tag and push it:

```sh
git switch -c release-0.9.x v0.9.0
git push -u origin release-0.9.x
```

Land the fix on master first, then cherry-pick it onto the release branch and
push the branch. If master changed too much for the cherry-pick, fix on the
branch and port the fix forward to master at once. Then, on the branch:

```sh
make rc VERSION=0.9.1
make release VERSION=0.9.1
```

The tags push alone and no branch ever gets a release commit. A hotfix of an
old line does not take `releases/latest`, and the site workflow skips any
release that is not the latest, so the site installer, `omatalk upgrade`, and
the site keep serving the newest line.

The plugin has its own `make bump` / `make release` (`manifest.json`; test,
validate, commit, push, gh), run in its own clone. Its tooling is unchanged.
Plugin RC work stays on a plugin branch and never on plugin master, because
`omarchy plugin add` and `omarchy plugin update` use mutable HEAD. A Daemon
release never requires a plugin release, and the reverse.

The release tarball `omatalk-x86_64.tar.gz` holds the `omatalk` binary,
`omatalk.service`, and `LICENSE`. The binary dlopens `onnxruntime-cpu` and
`espeak-ng` only in `omatalk daemon` and `omatalk render`. `install.sh`
installs both through `omarchy pkg add`.

`scripts/build.sh` runs from `scripts/release.sh`. Do not add a Make target
for it. `scripts/dev-install.sh` installs a local release
build over `~/.local/bin/omatalk` for dogfooding. It is not a Make target
either.

The plugin never installs the Daemon. Its setup screen links to the site
install page and shows no command. `omatalk upgrade` and the site curl fetch
`releases/latest/download/install.sh`, which build pins to that release's
tarball (`RELEASE_TAG` / `TARBALL_SHA256`).

Marketplace listing after a plugin SHA change:
`docs/agents/plugin-marketplace.md`.

## Agent skills

### Issue tracker

Issues live in GitHub Issues (`gh` CLI). See `docs/agents/issue-tracker.md`.

### Triage labels

Default five canonical triage role strings. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` at root + `docs/adr/`. See `docs/agents/domain.md`.

### Bar plugin tests

In the plugin clone: `make test` and `omarchy plugin validate .` must
pass (`make release` runs both). QML tests need `qmltestrunner` (skipped with a note when it is not
installed; required in plugin CI).

### Marketplace listing

Listed: https://omarchyplugins.com/plugin.html?id=zerobearing.omatalk
(approved on omacom/omarchy-plugin-marketplace#8407 at plugin v1.2.2). Each
plugin release files a `[Verify]:` issue for the plugin's `master` HEAD.
See `docs/agents/plugin-marketplace.md`.

### Tests

`cargo test` is the whole suite: unit and actor tests in `src/`, plus
black-box integration tests in `tests/` (`tests/installer/` is one binary
split by topic). Those drive the built binary and the shell scripts
(`install.sh`, `uninstall.sh`, `scripts/build.sh`, `public/*.sh`) in temp
dirs, with the stubs in `tests/fakes/system` first on `PATH` and the fake
engine (`OMATALK_TEST_FAKE_ENGINE=1`). Shared helpers are in `tests/common/`.

```sh
cargo test
```

Tests that need the real onnxruntime, espeak-ng, or models are `#[ignore]`.
Run them with `cargo test --release -- --ignored`.

### Shell scripts

Writing or editing a bash/shell script: follow `docs/agents/code_style.md`.

### Lint & format

Required before opening a PR:

```sh
make lint     # cargo fmt --check, cargo clippy --all-targets -D warnings
make format   # cargo fmt
```

A local pre-commit hook backstops this (one-time install:
`git config core.hooksPath .githooks`), but it isn't active on a fresh
clone — running `make lint`/`make format` yourself is the actual
compliance step, not the hook.
