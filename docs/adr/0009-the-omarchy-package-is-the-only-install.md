# The Omarchy package is the only install

Through 0.10, Omatalk installed with a curl script. `install.sh` downloaded a
pinned tarball into `~/.local/bin`, wrote a user unit into
`~/.config/systemd/user`, fetched the models, and added the plugin.
`omatalk upgrade` fetched the next release's script and ran it. Omarchy already
has a package manager, an update command, and a package repository (OPR), and
the curl install bypassed all three. A user's `omarchy update` never touched
Omatalk.

Now `omarchy pkg add omatalk-bin && omatalk setup` is the one install, and
`omarchy update` is the one upgrade.

- pacman owns `/usr/bin/omatalk` and `/usr/lib/systemd/user/omatalk.service`,
  with `ExecStart=/usr/bin/omatalk daemon`.
- `omatalk setup` does what pacman cannot do for one user: models, enabling the
  unit, the F8 binding, the bar plugin link, and a welcome speech. `setup.sh` holds
  the model pins. Every step checks whether it is done and skips itself, so a
  rerun changes nothing it already did. Two things repeat: a rerun offers the
  F8 binding again if it was declined, and it speaks the welcome every time.
  Setup keeps no state to remember a no. The binary embeds the script, as it
  does `uninstall.sh`.
- The bar plugin ships in the package at
  `/usr/share/omatalk/omarchy-plugin/zerobearing.omatalk/`, the way voxtype-bin
  and superwhisper-bin ship theirs. Its source moved into this repository under
  `plugin/zerobearing.omatalk/` with its history, so it always matches the
  Daemon it came with and updates with pacman. `omatalk setup` links it into
  `~/.config/omarchy/plugins/`, rescans, and enables it. A curl-era git
  checkout at that path is deleted first, and the shell restarts.
- `omatalk setup` also migrates a curl install: it disables and deletes the
  curl-era unit, binary, and v0.5 leftovers, and keeps models, config, and
  the F8 line. Curl users should still run the shim: it calls
  `/usr/bin/omatalk setup` by path, while a bare `omatalk setup` can reach the
  old `~/.local/bin/omatalk` when that comes first on PATH or is hashed, and
  that binary has no `setup`. The plugin ID stays
  `zerobearing.omatalk`, so a saved bar placement keeps working. The plugin
  calls `/usr/bin/omatalk` only, and `scripts/build.sh` stamps the release
  version into its manifest. This supersedes ADR 0004.
- `omatalk upgrade` is deleted. `omatalk uninstall` removes the user's state,
  then runs `omarchy pkg drop omatalk-bin`.
- The package ships two alpm hooks. `remove-user-services.hook` stops and
  removes the user unit for every account before removal, and aborts the
  removal if it cannot. It also deletes each user's plugin link, but only a link
  into `/usr/share/omatalk/`. `restart-user-services.hook` runs `try-restart` on each
  online user manager after an upgrade, so nobody keeps running a deleted
  binary. A failed restart never fails the update. The hooks and their scripts
  live in `packaging/dev/` and are copied into the recipe in omacom/omarchy-pkgs.
- `install.sh` becomes a migration shim. It installs the package and execs
  `omatalk setup`, which does the migration. Old
  binaries' `omatalk upgrade` and the site's dispatcher both reach it through
  `releases/latest/download/install.sh`. If `omarchy pkg add` fails, it
  changes nothing. Retire the shim and `public/install.sh` at 1.0.
- The tarball stays a release asset: it is the source of the recipe. No
  release commit rewrites a pin any more. This supersedes the `install.sh` pin
  parts of ADR 0007. `uninstall.sh` stops being a release asset.

Why a shim and not nothing. Deleting `install.sh` makes every 0.x
`omatalk upgrade` fail with a curl 404 and leaves those users with no path. The
shim costs one small file and a dispatcher. That retirement date is a judgment
call, not a measured one.

Considered and rejected:

- **Keep both install kinds.** Two owners for one command, and a PATH order to
  reason about. The operator chose the package.
- **Keep the plugin in its own repo on the marketplace.** The plugin installed
  from mutable `master`, so a plugin change had to land before the package that
  depended on it, and the two could drift.
- **Copy the plugin into `~/.config` instead of linking it.** A copy goes stale
  on every `omarchy update`. A link follows pacman.
- **Setup in a pacman `.install` hook.** It runs as root, so it cannot fetch
  per-user models or enable user units.
- **Enable the unit globally in the package.** It would start the Daemon
  before the models exist, and `Restart=always` would loop.
- **The Daemon notices its replaced binary and re-execs.** That logic would live
  in the hot process for a job pacman already has a hook for.

Consequences: a dev build is a package too. `scripts/dev-install.sh` builds
`packaging/dev` at `<newest tag>.r<commits>.g<sha>`, which sorts above that
tag and below the next RC or release, so `omarchy update` leaves it alone until
one ships. A curl-era user who has not migrated keeps the v1.2.2 plugin
checkout, which still calls `~/.local/bin/omatalk`, until `omatalk setup` replaces it.
The Daemon's version is not on the socket, so after an upgrade the check is
that `readlink /proc/<MainPID>/exe` is `/usr/bin/omatalk` without `(deleted)`.
