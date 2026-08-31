# Omatalk

Local text-to-speech for Omarchy: hotkey → the machine speaks your selected text.
See `CONTEXT.md` for domain language.

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
