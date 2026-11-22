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
  unit, the F8 binding, the bar plugin, and a welcome speech. `setup.sh` holds
  the model pins. Every step checks whether it is done and skips itself, so a
  rerun changes nothing and asks nothing. The binary embeds the script, as it
  does `uninstall.sh`.
- `omatalk upgrade` is deleted. `omatalk uninstall` removes the user's state,
  then runs `omarchy pkg drop omatalk-bin`.
- The package ships two alpm hooks. `remove-user-services.hook` stops and
  removes the user unit for every account before removal, and aborts the
  removal if it cannot. `restart-user-services.hook` runs `try-restart` on each
  online user manager after an upgrade, so nobody keeps running a deleted
  binary. A failed restart never fails the update. The hooks and their scripts
  live in `packaging/dev/` and are copied into the recipe in omacom/omarchy-pkgs.
- `install.sh` becomes a migration shim. It installs the package, deletes the
  curl-era binary, unit, and v0.5 leftovers, and execs `omatalk setup`. Old
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
- **Setup in a pacman `.install` hook.** It runs as root, so it cannot fetch
  per-user models or enable user units.
- **Enable the unit globally in the package.** It would start the Daemon
  before the models exist, and `Restart=always` would loop.
- **The Daemon notices its replaced binary and re-execs.** That logic would live
  in the hot process for a job pacman already has a hook for.

Consequences: a dev build is a package too. `scripts/dev-install.sh` builds
`packaging/dev` at `<latest stable>.r<commits>.g<sha>`, which sorts above that
release and below the next, so `omarchy update` leaves it alone until a newer
release ships. The bar plugin finds `/usr/bin/omatalk` first and
`~/.local/bin/omatalk` second until the shim retires, in a plugin release of its
own. The Daemon's version is not on the socket, so after an upgrade the check is
that `readlink /proc/<MainPID>/exe` is `/usr/bin/omatalk` without `(deleted)`.
