# Omatalk

Local text-to-speech for Omarchy: hotkey → the machine speaks your selected text.
See `CONTEXT.md` for domain language.

This repository is the Daemon, CLI, and site. Python lives in `daemon/`.
The installed CLI is still `omatalk` / `omatalkd`. The listed bar plugin is
https://github.com/zerobearing2/omarchy-omatalk-plugin, a separate repo with
its own release. Work on it in a plain clone of that repo
(`~/Work/omarchy-omatalk`). The installed plugin at
`~/.config/omarchy/plugins/zerobearing.omatalk` is a git clone too. Its
agent docs stay here, because the listed tree must not track `AGENTS.md`,
`CONTEXT.md`, or `docs/agents/`.

## Release

Bump edits the version files and does not commit. Release does build, verify, commit,
push, and `gh release create`. It will not clobber an existing tag.
GitHub Actions only runs tests on push.

```sh
make bump                 # pyproject.toml + uv.lock; VERSION=x.y.z to set it
make release              # master only; build, verify, commit, push, gh
```

The plugin has the same `make bump` / `make release` (`manifest.json`;
test, validate, commit, push, gh), run in its own clone. A Daemon release
never requires a plugin release, and the reverse.

Runtime dependencies install from the hashed `requirements.txt` shipped in
the tarball. After changing dependencies in `pyproject.toml`: `uv lock`, then
re-export (`tests/test_requirements.py` fails with the exact command).

Build and verify live in `scripts/` and run from release. Do not add
Make targets for them.

The plugin never installs the Daemon. Its setup screen shows the site curl
command. `omatalk upgrade` and the site curl fetch
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

Run with uv (creates the dev env on demand):

```sh
uv run --group dev pytest tests/
```

### Shell scripts

Writing or editing a bash/shell script: follow `docs/agents/code_style.md`.

### Lint & format

Required before opening a PR:

```sh
make lint
make format
```

A local pre-commit hook backstops this (one-time install:
`git config core.hooksPath .githooks`), but it isn't active on a fresh
clone — running `make lint`/`make format` yourself is the actual
compliance step, not the hook.
