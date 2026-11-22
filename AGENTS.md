# Omatalk

Local text-to-speech for Omarchy: a hotkey makes the machine speak your
selected text. Use the terms in `GLOSSARY.md`. The Omarchy package
`omatalk-bin` is the only install (`docs/adr/0009-the-omarchy-package-is-the-only-install.md`).

## Repository map

- `src/` is one Rust crate and one binary. `omatalk daemon` is the Daemon, and
  every other argv is the CLI.
- `src/protocol.rs` is the only module that spells socket words. Keep `follow`
  and the state words `idle`, `speaking`, and `error` unchanged, because plugin
  v1.2.2 depends on them.
- `public/` is the site. See [Site](#site).
- `tools/parity/` holds standalone uv scripts, outside the test suite, that
  regenerate reference data from Python spaCy and misaki. Run one directly:
  `tools/parity/dump_misaki.py lines.txt out.jsonl`. See `data/NOTICE.md`.
- `setup.sh` is the per-user work the package cannot do: models, unit enable,
  F8 binding, bar plugin, welcome speech. `omatalk setup` execs the copy
  embedded in the binary, like `omatalk uninstall` does with `uninstall.sh`. The
  model pins live in `setup.sh`.
- `install.sh` is a migration shim for curl installs, not an installer. It runs
  `omarchy pkg add omatalk-bin`, deletes the curl-era binary and unit, and execs
  `omatalk setup`. `public/install.sh` dispatches to the latest release's copy.
  Retire both at 1.0.
- `packaging/dev/` is a PKGBUILD and the two alpm hooks with their scripts
  (`package-remove`, `package-restart`). The recipe in omacom/omarchy-pkgs
  carries the same hooks. A hook change lands in both.
- `scripts/dev-install.sh` dogfoods this checkout: it builds `packaging/dev` as
  `omatalk-bin` at `<latest stable>.r<commits>.g<sha>` and runs
  `sudo pacman -U` on it. It needs models from an earlier `omatalk setup`.
  `omarchy pkg drop omatalk-bin && omarchy pkg add omatalk-bin` goes back.
- `scripts/build.sh` runs from `scripts/release.sh`. Keep both build scripts
  out of the Makefile.

The bar plugin is the separate repo
https://github.com/zerobearing2/omarchy-omatalk-plugin, with its own release.
Change it in a plain clone at `~/Work/omarchy-omatalk-plugin`. The installed
plugin at `~/.config/omarchy/plugins/zerobearing.omatalk` is a git clone too.
Keep its agent docs here, because the listed tree must not track `AGENTS.md`,
`GLOSSARY.md`, or `docs/agents/`. The plugin never installs the Daemon. Its
setup screen links to the site install page and shows no command.

## Release

`master` is the dev line, and pushing to it publishes nothing. A release is a
tag plus its GitHub Release (`docs/adr/0007-releases-are-tags-from-master.md`).

```sh
make rc VERSION=0.10.0        # v0.10.0-rc.N, a GitHub prerelease
make release VERSION=0.10.0   # v0.10.0; needs an RC of this exact HEAD
```

Both run `scripts/release.sh`. It requires branch `master` or `release-X.Y.x`,
`VERSION` as `x.y.z` (`X.Y.z` on a release branch), a clean tree, and HEAD equal
to `origin/<branch>`. `make release` also requires that the highest
`v$VERSION-rc.*` tag was built from HEAD (`tag^` equals HEAD). If the branch
moved, cut another RC.

The script builds in a temporary worktree. It sets the version, runs
`cargo test --locked`, `make lint`, and `scripts/build.sh`, commits
`Release vTAG`, tags the commit, pushes only the tag, and runs
`gh release create --verify-tag`. A release commit never goes on a branch.

Keep master's `Cargo.toml` version at `0.0.0-dev`, so a local build never claims
to be a release. Nothing is pinned in a release commit: the tarball is the
source of the `omatalk-bin` recipe in omacom/omarchy-pkgs, whose `pkgver` and
tarball checksum change per release.

To test an RC, point a local `omatalk-bin` recipe at the RC's tarball, build it
with `makepkg`, and install it with `sudo pacman -U`. `scripts/dev-install.sh`
does the same for this checkout.

`releases/latest` feeds the site's install.sh dispatcher, which old binaries'
`omatalk upgrade` also read. Only `scripts/release.sh` sets it, and it verifies
the result. RCs never take it. A
stable release takes it only when its version is numerically higher than the
current latest. If the script cannot read it (anything but a 404), it refuses
before it pushes.

The release assets are the tarball `omatalk-x86_64.tar.gz` with its `.sha256`,
and the shim `install.sh`. The tarball holds the `omatalk` binary,
`omatalk.service`, and `LICENSE`. The binary dlopens `onnxruntime-cpu` and
`espeak-ng` only in `omatalk daemon` and `omatalk render`. The package depends
on both.

### Hotfix an old version

Use a `release-X.Y.x` branch, so unreleased master code stays out.

1. Create it once from the tag:
   `git switch -c release-0.9.x v0.9.0 && git push -u origin release-0.9.x`.
2. Land the fix on master, cherry-pick it onto the branch, and push the branch.
   If the cherry-pick is too hard, fix on the branch and port to master at once.
3. On the branch, run `make rc VERSION=0.9.1`, then `make release VERSION=0.9.1`.

A hotfix of an old line does not take `releases/latest`, so the site and its
install.sh dispatcher keep serving the newest line.

### Plugin release

Run `make bump`, then `make release` (`manifest.json`; test, validate, commit,
push, `gh`) in the plugin clone. These targets exist only there. Keep plugin RC
work on a plugin branch, because `omarchy plugin add` and `update` use mutable
HEAD. A Daemon release never requires a plugin release, and the reverse.

After a plugin SHA change, follow `docs/agents/plugin-marketplace.md`.

### Site

`.github/workflows/pages.yml` deploys when a non-prerelease that is the current
`releases/latest` is published, or on `gh workflow run pages.yml`. To publish a
site-only fix, run that dispatch. It deploys everything in `public/` on master.

## Debug a user's install

1. Run these commands on the affected machine:

   ```sh
   omatalk version
   omatalk status
   pacman -Q omatalk-bin
   systemctl --user status omatalk.service --no-pager -l
   journalctl --user -u omatalk.service -b --no-pager -n 80
   readlink /proc/$(systemctl --user show -p MainPID --value omatalk.service)/exe
   stat -c '%A %U:%G %n' "${XDG_RUNTIME_DIR:-/run/user/$UID}/omatalk/omatalk.sock" \
     ~/.config/omarchy/plugins/zerobearing.omatalk/manifest.json \
     ~/.config/omarchy/plugins/zerobearing.omatalk/BarWidget.qml
   ss -xap | grep -E 'omatalk|quickshell'
   ls ~/.local/bin/omatalk ~/.config/systemd/user/omatalk.service
   omarchy plugin list --json | grep -C 3 'zerobearing.omatalk'
   omarchy-shell shell listPlugins | grep -C 3 'zerobearing.omatalk'
   ```

   The `readlink` must print `/usr/bin/omatalk` with no `(deleted)`. A
   `(deleted)` means the Daemon still runs an old binary: run
   `systemctl --user restart omatalk.service`. The two `ls` paths are the
   curl-era install. If either exists, run `bash -c "$(curl -fsSL https://omatalk.zerobearing.com/install.sh)"`
   to migrate.

2. Check each layer: the service and `omatalk status`, the socket and its
   Quickshell peer, whether the plugin is present and enabled, and QML errors in
   the logs. With no peer and a working status, try `omarchy restart shell`.
3. Ask before you change anything. Preserve `~/.config/omatalk/config.toml`,
   `~/.config/hypr/bindings.lua`, and the Omarchy shell layout.

Done when `omatalk status` works and a Quickshell peer is on the socket.

## Agent skills

### Issue tracker

Issues live in GitHub Issues (`gh` CLI). See `docs/agents/issue-tracker.md`.

### Triage labels

Default five canonical triage role strings. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` at root + `docs/adr/`. See `docs/agents/domain.md`.

### Bar plugin tests

In the plugin clone, `make test` and `omarchy plugin validate .` must pass.
`make release` runs both. QML tests need `qmltestrunner`: they skip with a note
when it is missing, and plugin CI requires it.

### Marketplace listing

Listed at https://omarchyplugins.com/plugin.html?id=zerobearing.omatalk. The
plugin must show no install command. File a `[Verify]:` issue for the plugin's
`master` HEAD after each plugin release. See `docs/agents/plugin-marketplace.md`.

### Tests

`cargo test` is the whole suite. It covers unit and actor tests in `src/`, plus
black-box tests in `tests/` (`tests/installer/` is one binary split by topic).
The black-box tests drive the built binary and the shell scripts in temp dirs,
with the stubs in `tests/fakes/system` first on `PATH` and the fake engine
(`OMATALK_TEST_FAKE_ENGINE=1`). Shared helpers are in `tests/common/`.

Tests that need the real onnxruntime, espeak-ng, or models are `#[ignore]`.
Run them with `cargo test --release -- --ignored`.

`tests/cli.rs` and `tests/site.rs` read `README.md`. They pin the Config table
and the install, uninstall, plugin, and render commands, so change those
together with the code or the site.

### Shell scripts

Writing or editing a bash or shell script: follow `docs/agents/code_style.md`.

### Lint & format

Run `make lint` before you open a PR:

```sh
make lint     # cargo fmt --check, cargo clippy --all-targets -D warnings
make format   # cargo fmt
```

The pre-commit hook needs a one-time `git config core.hooksPath .githooks`, so
a fresh clone does not have it. GitHub Actions runs `make lint` and the tests on
pushes to master and on pull requests. It never builds or publishes a release.
