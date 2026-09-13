# Marketplace listing

Plugin ID `zerobearing.omatalk` is permanent. Repo:
https://github.com/zerobearing2/omarchy-omatalk-plugin (this workspace's
`plugin/` submodule).

Listings live in `omacom/omarchy-plugin-marketplace`. Validation and the
Automated Security Baseline scan an exact 40-character commit. They do not
execute plugin code. Approval is maintainer-only.

Read [SUBMISSION.md](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md),
[SECURITY.md](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SECURITY.md),
and [VERIFICATION.md](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/VERIFICATION.md)
when the recipe below is not enough.

## Until the first listing

Submission is
https://github.com/omacom/omarchy-plugin-marketplace/issues/4712
Title stays `[Plugin]: Omatalk`.

If `git -C plugin rev-parse HEAD` is not the commit in the bot's latest
validation comment, edit that issue (same six headings, all five checkboxes
checked). Editing retriggers detection. A comment does not.

Keep tags as `bar, media, quickshell`, category `Productivity`, and
`manual-setup` as a stated fact in Maintainer notes. Install still fetches
the pinned `install.sh` (URL + SHA-256 in `plugin/Panel.qml`); the script
still installs the current GitHub release tarball.

Done when the bot validation comment names current `plugin/` HEAD, the
baseline is `passed` (or `review-required` with `installer` only), and
labels include `validated` and `manual-setup`. Then stop. A maintainer
applies `approved-and-verified`.

## After listing, each plugin release

`make release` (when `install.sh` changed) or `make plugin-bump` then
`git -C plugin push` and `make plugin-release` stay as in `AGENTS.md`.
Then file a **new** issue — not #4712.

```sh
SHA=$(git -C plugin rev-parse HEAD)
VERSION=$(jq -r .version plugin/manifest.json)
```

`$SHA` must be 40 hex characters of the commit that was pushed.

Form: https://github.com/omacom/omarchy-plugin-marketplace/issues/new?template=verify-plugin.yml

Select **Verify and publish a newer upstream commit**. Leave the standard
installation box unchecked.

```md
### Verification action

Verify and publish a newer upstream commit

### Plugin ID

zerobearing.omatalk

### Repository URL

https://github.com/zerobearing2/omarchy-omatalk-plugin

### Target commit

$SHA

### Verification acknowledgment

- [x] I understand that only the exact target commit can become a verified marketplace snapshot and that verification is not a security audit.

### Standard installation acknowledgment

- [ ] I confirm that this listed root plugin supports the standard Omarchy installation path and does not require manual setup.
```

Title: `[Verify]: Omatalk $VERSION`. Create only after the owner confirms.

CLI:

```sh
gh issue create \
  --repo omacom/omarchy-plugin-marketplace \
  --title "[Verify]: Omatalk $VERSION" \
  --body-file /tmp/omarchy-plugin-verify.md
```

If the bot fails, edit that Verify issue. A later `plugin/` `master` push
without a new Verify issue leaves the catalog `Update unverified`.

Done when the bot validation comment names `$SHA`. Then stop. A maintainer
applies `approved-and-verified` last. `manual-setup` stays.

After a plugin SHA you want recorded here: `git add plugin && git commit`.

## Snapshot re-check

Use the same form, **Verify the currently listed snapshot**, only when the
target is the listing's recorded `listingValidatedCommit` (from the plugin
detail page), not current HEAD. A `review-required` result needs
`maintainer-verified` on that issue, not `approved-and-verified`.

## Guardrails

- Keep `zerobearing.omatalk`. Update README and commands in the same change
  if it ever must move, and only before the first listing.
- Keep `install.sh` in this repository, not in `plugin/`.
- Re-pin when `install.sh` itself changes (`make release` does this after
  the Daemon push). Not for every Daemon tarball.
- `omarchy plugin add` / `update` clone mutable HEAD, not the verified SHA.
- Agent docs stay in this repository. The `plugin/` git tree is the listed
  snapshot.
