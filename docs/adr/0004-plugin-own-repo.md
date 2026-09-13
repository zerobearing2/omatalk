# Plugin lives in its own repository

The bar widget is an Omarchy plugin. It is written, tested, and versioned in
https://github.com/zerobearing2/omarchy-omatalk-plugin. This repository is
the Daemon, the CLI, and the site.

`omarchy plugin add` clones that git tree. `omarchy plugin update` pulls QML.
`omarchy plugin remove` unloads the megaphone. This installer calls those
commands when the plugin is missing or is a leftover file copy; it does not
ship QML in the release tarball and does not copy into
`~/.config/omarchy/plugins/`.

A Daemon upgrade (`omatalk upgrade` / the site curl) therefore cannot rewrite
plugin code. QML commits still go to omarchy-omatalk-plugin.

For development this repository nests that checkout as the `plugin/`
submodule. The tarball still does not include QML. The plugin ships a
copy of this repo's `install.sh` for the panel Install button. That
script downloads a pinned Daemon release tarball (`RELEASE_TAG` and
`TARBALL_SHA256`). `make plugin-release` copies that script into
`plugin/`; `make release` does not.
