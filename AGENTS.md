# Omatalk

Local text-to-speech for Omarchy: hotkey → the machine speaks your selected text.
See `CONTEXT.md` for domain language.

This repository is the Daemon, CLI, and site. Python lives in `daemon/`.
The installed CLI is still `omatalk` / `omatalkd`. The listed bar plugin is
the `plugin/` submodule (https://github.com/zerobearing2/omarchy-omatalk-plugin).
Commit QML in `plugin/`. Agent docs stay here. Start Grok at this root.

## Release

`make bump` (or `make bump VERSION=x.y.z`), push, then `make release` — see
the Makefile's comments for the full mechanics.

### Re-pin the bar plugin — only when install.sh actually changed

`plugin/Panel.qml` pins `install.sh` to an exact commit + SHA-256. That pin
freezes the installer script, not the Daemon version: `install.sh` already
fetches the latest release at install time, so most Daemon releases need no
plugin action.

After releasing, check whether `install.sh` changed since the plugin's
current pin:

```sh
pinned=$(grep -oP '(?<=omatalk/)[0-9a-f]{40}(?=/install\.sh)' plugin/Panel.qml)
git diff --quiet "$pinned" HEAD -- install.sh \
  && echo "no re-pin needed" \
  || echo "install.sh changed — make plugin-pin-release"
```

If it changed: `make plugin-pin-release`, `git -C plugin push`, then
`make plugin-release`. Then record the new submodule SHA here
(`git add plugin && git commit`).

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
