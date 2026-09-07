# Omatalk

Local text-to-speech for Omarchy: hotkey → the machine speaks your selected text.
See `CONTEXT.md` for domain language.

This repository is the Daemon, CLI, and site. Python lives in `daemon/`.
The installed CLI is still `omatalk` / `omatalkd`. The bar plugin is
https://github.com/zerobearing2/omarchy-omatalk-plugin.

## Release

`make bump` (or `make bump VERSION=x.y.z`), push, then `make release` — see
the Makefile's comments for the full mechanics.

### Re-pin the bar plugin — only when install.sh actually changed

The bar plugin (zerobearing2/omarchy-omatalk-plugin) pins `install.sh` to an
exact commit + SHA-256 (marketplace security requirement — no unverified
`curl | bash`). That pin freezes the *installer script*, not the Daemon
version: `install.sh` already fetches the latest release at install time, so
most Daemon releases need no plugin action. Re-pinning when nothing changed
just produces a needless plugin release.

After releasing, check whether `install.sh` changed since the plugin's
current pin:

```sh
pinned=$(git -C <plugin-checkout> show HEAD:Panel.qml \
  | grep -oP '(?<=omatalk/)[0-9a-f]{40}(?=/install\.sh)')
git diff --quiet "$pinned" HEAD -- install.sh \
  && echo "no re-pin needed" \
  || echo "install.sh changed — re-pin the plugin"
```

If it changed: in the plugin checkout, `make pin-release` (pins + bumps
plugin version + commits — see its `AGENTS.md` "Installer pin"), push, then
`make release` there too.

## Agent skills

### Issue tracker

Issues live as local markdown under `.scratch/<feature>/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five canonical triage role strings. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` at root + `docs/adr/`. See `docs/agents/domain.md`.

### Tests

Run with uv (creates the dev env on demand):

```sh
uv run --group dev pytest tests/
```
