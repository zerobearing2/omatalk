# Marketplace listing

Plugin ID `zerobearing.omatalk` is permanent. Repo:
https://github.com/zerobearing2/omarchy-omatalk-plugin (a separate repo;
run the commands below in a clone of it).

Listings live in `omacom/omarchy-plugin-marketplace`. Validation and the
Automated Security Baseline scan an exact 40-character commit. They do not
execute plugin code. Approval is maintainer-only.

Read [SUBMISSION.md](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md),
[SECURITY.md](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SECURITY.md),
and [VERIFICATION.md](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/VERIFICATION.md)
when the recipe below is not enough.

## Until the first listing

The first submission, #4712, was closed for inactivity after a security
review. Its objection was the old installer path, which fetched a mutable
tarball and checksum. The plugin now ships no installer at all. File a new
`[Plugin]: Omatalk` issue with the submit-plugin form, and write fresh
maintainer notes against the current `master` SHA. Do not reuse #4712.

Keep tags as `bar, media, quickshell`, category `Productivity`, and
`manual-setup` as a stated fact in Maintainer notes. Setup is manual: the
panel shows `curl -fsSL https://omatalk.zerobearing.com/install.sh | bash`
with a copy button. The plugin downloads and executes nothing. It calls the
`omatalk` CLI once `~/.local/bin/omatalk` exists.

Done when the bot validation comment names current plugin `master` HEAD,
the baseline is `passed`, and labels include `validated` and
`manual-setup`. Then stop. A maintainer applies `approved-and-verified`.

## After listing, each plugin release

Release as in the plugin README (bump `manifest.json`, commit, push,
`gh release create`). Then file a **new** Verify issue.

```sh
SHA=$(git rev-parse HEAD)
VERSION=$(jq -r .version manifest.json)
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

If the bot fails, edit that Verify issue. A later plugin `master` push
without a new Verify issue leaves the catalog `Update unverified`.

Done when the bot validation comment names `$SHA`. Then stop. A maintainer
applies `approved-and-verified` last. `manual-setup` stays.

## Snapshot re-check

Use the same form, **Verify the currently listed snapshot**, only when the
target is the listing's recorded `listingValidatedCommit` (from the plugin
detail page), not current HEAD. A `review-required` result needs
`maintainer-verified` on that issue, not `approved-and-verified`.

## Guardrails

- Keep `zerobearing.omatalk`. Update README and commands in the same change
  if it ever must move, and only before the first listing.
- The plugin must never ship or run an installer. Its `tests/run.sh`
  fails if `install.sh` or `uninstall.sh` is in the tree.
- `omarchy plugin add` / `update` clone mutable HEAD, not the verified SHA.
- Agent docs stay in this repository. The plugin repo's git tree is the
  listed snapshot.
