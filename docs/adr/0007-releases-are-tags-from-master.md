# Releases are tags from master

Until 0.9.0 a release was a bump, a commit, and a push to `master`, then a
GitHub Release. Anything that reached `master` was one command from stable
users, and the site deployed on every push that touched `public/`. Merging a
PR and shipping it were the same act, so a change could not land to be tested
without risking everyone.

Now `master` is the dev line and pushing to it publishes nothing. A release is
a tag plus its GitHub Release, built by `scripts/release.sh` in a temporary
worktree from a clean master that equals `origin/master`.

- `make rc VERSION=0.10.0` publishes `v0.10.0-rc.N` as a prerelease. N is one
  past the highest RC tag on the remote. `releases/latest`, which the site
  installer and `omatalk upgrade` read, does not move.
- `make release VERSION=0.10.0` publishes `v0.10.0`, as latest when it is the
  highest version. It refuses unless an RC exists and the highest RC's parent
  commit is HEAD, so the real release is the code last tested.

Why tag-only. The version in `Cargo.toml` and the pin in `install.sh` must
differ per release, and the pin needs the tarball's digest, which exists only
after the build. Committing those to `master` is what coupled shipping to
merging. The release commit now exists only under the tag and nothing writes
to `master` or any branch. The cost is that `master`'s version and pin do not
track releases. Each tag's commit holds them.

Why RC-gated, and why only two kinds of branch. A release from an arbitrary
branch or a dirty tree is not reproducible, and a stable release built from
different code than its RC would ship what nobody ran. Requiring `tag^` to
equal HEAD makes the RC-to-stable step a re-tag of tested code. If the branch
moved, cut another RC. Releases are cut from `master` or from a
`release-X.Y.x` branch, where `VERSION` must be `X.Y.z`. The script accepts
nothing else, and the branch must equal its `origin` copy.

Release branches exist for hotfixes. `release-0.9.x` is created from `v0.9.0`
and holds only what that line needs, so unreleased master code never rides
along. The fix lands on master first and is cherry-picked to the branch. If
master changed too much, fix on the branch and port forward at once. Release
branches carry no release commits either: the same tag-only script runs there.

Two guards keep an old line from reaching users who are on a newer one.
`releases/latest` feeds the site installer and `omatalk upgrade`, and GitHub
sets it by publish order, so a late 0.9.1 would otherwise displace 0.10.0. The
script reads the current latest first, compares versions numerically, and
passes `--latest` only for a higher version, `--latest=false` otherwise. It
checks the result afterward and exits non-zero on a mismatch. It refuses when
the lookup fails with anything but a 404, since a 404 means no release exists
and anything else means the answer is unknown. The same lookup is what makes
the site safe. `pages.yml` runs a gate job that reads `releases/latest` with
the workflow token and deploys only when its tag is the published release's.
Manual dispatch still deploys. The gate re-reads a few times first, because
the release event can fire before the latest pointer settles.

The site follows the same rule. `pages.yml` runs on a published
non-prerelease, or by hand with `gh workflow run pages.yml`. A site-only fix
therefore needs that dispatch, which deploys everything in `public/` on
`master`. The Omarchy plugin directory is untouched by Daemon releases. Plugin
RC work stays on a plugin branch and never on plugin master, because
`omarchy plugin add` and `update` clone mutable HEAD.

Considered and rejected:

- **Dev versions on master (`0.10.0-dev.N`).** Every merge would still be a
  release candidate by default, and bumping the file is the commit-to-master
  step this removes. No prerelease was ever published, so nothing needed it.
- **A long-lived `rc` branch.** Two lines to keep in sync, with merges between
  them, to hold what a tag already holds.
- **`omatalk upgrade --rc`.** A new CLI surface and a way for a user to land
  on an untested channel by accident. Testers run the RC's install command.
- **Building in Actions.** The workflow would need permission to push tags
  and create releases, and the tarball digest pinned in `install.sh` would come
  from a build the maintainer never ran. Today CI only tests and lints
  (`AGENTS.md`), and the maintainer's `make rc` is the build that ships.

Consequences: a hotfix is a release branch plus a cherry-pick, so the fix
exists twice and can drift if it is not ported or picked promptly. A fix
released from an old line is not offered to users on the newer one by
`releases/latest`; they get it from their own line's next release. `master`'s
`Cargo.toml` version and `install.sh` pin are stale by design, and
`dev-install.sh` reads the pin only for model file names.
