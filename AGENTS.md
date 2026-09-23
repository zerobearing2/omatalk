# Omatalk

Local text-to-speech for Omarchy: hotkey → the machine speaks your selected text.
See `CONTEXT.md` for domain language.

This repository is the Daemon, CLI, and site. Python lives in `daemon/`.
The installed CLI is still `omatalk` / `omatalkd`. The listed bar plugin is
the `plugin/` submodule (https://github.com/zerobearing2/omarchy-omatalk-plugin).
Commit QML in `plugin/`. Agent docs stay here. Start Grok at this root.
After clone or pull: `git submodule update --init`.

## Release

Daemon scripts have no prefix; `plugin-*` is plugin-only. Bump edits the
version file and does not commit. Release does build, verify, commit,
push, and `gh release create`. It will not clobber an existing tag.
GitHub Actions only runs tests on push.

```sh
make bump                 # pyproject.toml + uv.lock; VERSION=x.y.z to set it
make release              # master only; build, verify, commit, push, gh

make plugin-bump          # plugin/manifest.json only
make plugin-release       # plugin/ on master (git -C plugin switch master);
                          # commits + pushes the submodule pointer here;
                          # Daemon RELEASE_TAG must already exist on GitHub
```

Runtime dependencies install from the hashed `requirements.txt` shipped in
the tarball. After changing dependencies in `pyproject.toml`: `uv lock`, then
re-export (`tests/test_requirements.py` fails with the exact command).

Build and verify live in `scripts/` and run from release. Do not add
Make targets for them.

`plugin/install.sh` is a copy of this repo's `install.sh`. Edit the root
file only. Install in the panel downloads the Daemon tarball named in
`RELEASE_TAG` / `TARBALL_SHA256`. `omatalk upgrade` and the site curl
fetch `releases/latest/download/install.sh` (self-pinned to that release).

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

```sh
make plugin-test
```

`omarchy plugin validate plugin` must pass. QML tests need `qmltestrunner`
(skipped with a note when it is not installed; required in plugin CI).

### Marketplace listing

Until listed, re-validate by editing omacom/omarchy-plugin-marketplace#4712.
After listing, each plugin release files a `[Verify]:` issue for `plugin/`
HEAD. See `docs/agents/plugin-marketplace.md`.

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
