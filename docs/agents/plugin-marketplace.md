# Marketplace listing

Use this after every bar plugin release. Run the commands in a clone of
https://github.com/zerobearing2/omarchy-omatalk-plugin. Plugin ID
`zerobearing.omatalk` is permanent.

Listed at https://omarchyplugins.com/plugin.html?id=zerobearing.omatalk,
approved on `omacom/omarchy-plugin-marketplace#8407` at plugin v1.2.2
(`c0d145f`). Validation and the Automated Security Baseline scan an exact
40-character commit and do not run plugin code. Only a marketplace maintainer
can approve.

If this recipe is not enough, read
[SUBMISSION.md](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md),
[SECURITY.md](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SECURITY.md),
and [VERIFICATION.md](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/VERIFICATION.md).

## Rules for the plugin tree

- The panel links to https://omatalk.zerobearing.com/#install with no command.
  The plugin downloads, runs, displays, and copies no install command. Review
  closed #4712 over an installer in the plugin, and #8407's review objected to
  a `curl | bash` in the panel.
- The plugin calls the `omatalk` CLI once `~/.local/bin/omatalk` exists.
- The baseline scanner flags `curl | bash` in the plugin README, and `git fetch`
  followed by running scripts, including maintainer-only release scripts.
- The plugin never ships or runs an installer. Its `tests/run.sh` fails if
  `install.sh` or `uninstall.sh` is in the tree.
- The `manual-setup` label stays.
- Agent docs stay in this repository, because the plugin's git tree is the
  listed snapshot.

## Publish a plugin release

1. In the plugin clone, run `make bump`, then `make release`.
2. Read the commit and the version:

   ```sh
   SHA=$(git rev-parse HEAD)
   VERSION=$(jq -r .version manifest.json)
   ```

   `$SHA` must be the 40 hex characters of the pushed commit.

3. Write `/tmp/omarchy-plugin-verify.md` with `$SHA` filled in. It matches the
   [verify form](https://github.com/omacom/omarchy-plugin-marketplace/issues/new?template=verify-plugin.yml)
   with **Verify and publish a newer upstream commit** selected and the standard
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

4. After the owner confirms, file a **new** Verify issue:

   ```sh
   gh issue create \
     --repo omacom/omarchy-plugin-marketplace \
     --title "[Verify]: Omatalk $VERSION" \
     --body-file /tmp/omarchy-plugin-verify.md
   ```

   If the bot fails, edit that issue.

Done when the bot validation comment names `$SHA`. Then stop. A maintainer
applies `approved-and-verified` last.

A plugin `master` push without a new Verify issue leaves the catalog at
`Update unverified`, because `omarchy plugin add` and `update` clone mutable
HEAD, not the verified SHA.

## Re-check the listed snapshot

Use the same form with **Verify the currently listed snapshot**, only when the
target is the listing's `listingValidatedCommit` from the plugin detail page,
not current HEAD. A `review-required` result needs `maintainer-verified` on that
issue, not `approved-and-verified`.
