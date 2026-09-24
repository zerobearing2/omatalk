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

The plugin does not install the Daemon. Its setup screen links to the
site's install page (https://omatalk.zerobearing.com/#install), and the
user runs the command there in a terminal. The panel shows no command:
marketplace review objected to the plugin displaying a command for code
outside the reviewed snapshot. An earlier version
shipped a pinned copy of `install.sh` in the plugin for an Install button.
That tied every Daemon release to a plugin release and a marketplace
re-verification, so it was removed. The plugin is no longer a submodule
here. The two repositories release independently. The plugin only depends
on the `omatalk` CLI (`version`, `config get/set/voices`, `speak`).
