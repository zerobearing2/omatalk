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
- `make release VERSION=0.10.0` publishes `v0.10.0` as latest. It refuses
  unless an RC exists and the highest RC's parent commit is HEAD, so the real
  release is the code last tested.

Why tag-only. The version in `Cargo.toml` and the pin in `install.sh` must
differ per release, and the pin needs the tarball's digest, which exists only
after the build. Committing those to `master` is what coupled shipping to
merging. The release commit now exists only under the tag and nothing writes
to `master` or any branch. The cost is that `master`'s version and pin do not
track releases. Each tag's commit holds them.

Why master-only and RC-gated. A release from a branch or a dirty tree is not
reproducible, and a stable release built from different code than its RC
would ship what nobody ran. Requiring `tag^` to equal HEAD makes the
RC-to-stable step a re-tag of tested code. If master moved, cut another RC.

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

Consequences: there is no standalone hotfix while `master` holds unreleased
work. A fix to a shipped version must wait for `master` to be releasable, or
the unreleased work must be reverted first. `master`'s `Cargo.toml` version and
`install.sh` pin are stale by design, and `dev-install.sh` reads the pin only
for model file names.
