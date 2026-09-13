# Omatalk

Local text-to-speech for Omarchy: hotkey → the machine speaks your selected text.
See `CONTEXT.md` for domain language.

This repository is the Daemon, CLI, and site. Python lives in `daemon/`.
The installed CLI is still `omatalk` / `omatalkd`. The listed bar plugin is
the `plugin/` submodule (https://github.com/zerobearing2/omarchy-omatalk-plugin).
Commit QML in `plugin/`. Agent docs stay here. Start Grok at this root.
After clone or pull: `git submodule update --init`.

## Release

`make bump` (or `make bump VERSION=x.y.z`), push, then `make release` — see
the Makefile's comments for the full mechanics.

### Bar plugin pin

`plugin/Panel.qml` pins this repo's `install.sh` (commit URL + SHA-256 of
the local file). That freezes the installer script, not the Daemon tarball.

`make release` from pushed `master` re-pins the plugin (on `plugin` master)
only when HEAD's `install.sh` hash no longer matches the pin, then cuts the
Daemon release. Then `git -C plugin push`, `make plugin-release`, and record
the submodule SHA (`git add plugin && git commit`).

Plugin QML-only releases: `make plugin-bump`, `git -C plugin push`,
`make plugin-release`. Marketplace listing after that:
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
