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

Bump edits the version files and does not commit. Release does test, lint, build, commit, push,
and `gh release create`. It will not clobber an existing tag. A
version with a `-dev.N` suffix publishes a GitHub prerelease, so
`releases/latest` keeps serving the last stable release.
GitHub Actions runs `make lint` and the tests on pushes to master and on pull
requests. It never builds or publishes a release.

```sh
make bump                 # Cargo.toml + Cargo.lock; -dev.N counts up; VERSION=x.y.z[-dev.N] sets it
make release              # master only; test, lint, build, commit, push, gh
```

The plugin has the same `make bump` / `make release` (`manifest.json`;
test, validate, commit, push, gh), run in its own clone. A Daemon release
never requires a plugin release, and the reverse.

The release tarball `omatalk-x86_64.tar.gz` holds the `omatalk` binary,
`omatalk.service`, and `LICENSE`. The binary dlopens `onnxruntime-cpu` and
`espeak-ng` at Daemon startup only. `install.sh` installs both through
`omarchy pkg add`.

`scripts/build.sh` runs from release. Do not add a Make target
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

Issues live as local markdown under `.scratch/<feature>/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five canonical triage role strings. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` at root + `docs/adr/`. See `docs/agents/domain.md`.

### Bar plugin tests

In the plugin clone: `make test` and `omarchy plugin validate .` must
pass (`make release` runs both). QML tests need `qmltestrunner` (skipped with a note when it is not
installed; required in plugin CI).

### Marketplace listing

Not listed yet; #4712 was closed and a new submission is needed. After
listing, each plugin release files a `[Verify]:` issue for the plugin's
`master` HEAD. See `docs/agents/plugin-marketplace.md`.

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
