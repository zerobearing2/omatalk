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
plugin code. Plugin work does not land in this tree.
