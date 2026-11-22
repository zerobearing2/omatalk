# Marketplace listing

The plugin ships in the `omatalk-bin` package
(`docs/adr/0009-the-omarchy-package-is-the-only-install.md`), so there are no
plugin releases and no more `[Verify]:` issues.

The old listing is https://omarchyplugins.com/plugin.html?id=zerobearing.omatalk,
approved on `omacom/omarchy-plugin-marketplace#8407` at plugin v1.2.2
(`c0d145f`) from the repo https://github.com/zerobearing2/omarchy-omatalk-plugin.
That repo is frozen at its last commit, and `omarchy plugin add` from the
listing still installs that old checkout. `omatalk setup` replaces the checkout
with the packaged link.

## Retire the plugin repo and the listing

Do this after the first stable package release, so every new install already
gets the packaged plugin. It sends messages outside this repo, so it waits for
the operator's go.

1. In the plugin repo, commit a README that points to
   `omarchy pkg add omatalk-bin`, then archive the repo.
2. Ask the marketplace to delist `zerobearing.omatalk`, or to mark it as
   shipped by `omatalk-bin`.

Plugin ID `zerobearing.omatalk` is permanent: a saved bar placement depends on
it.

## Rules that still hold

- The plugin downloads, runs, displays, and copies no install command. Review
  closed #4712 over an installer in the plugin, and #8407's review objected to
  a `curl | bash` in the panel.
- The plugin calls `/usr/bin/omatalk` only.
