# clap parses argv

Reverses the "no clap" line in `docs/design/rust-rewrite.md`.

The hand-rolled parser printed one crammed usage line for every mistake, and
printed it on stderr with exit 2 even for `--help`. It had no per-command help
and never said what was wrong. Users expect `--help`, `-h`, `help <command>`,
`<command> --help`, `-V`, and an error that names the bad argument, as in every
other CLI.

clap's derive API gives all of that from the `Cli` enum's doc comments, so the
help cannot drift from the parser. The cost is about 0.5 MB of binary (19.5 to
20.0 MB) and a few seconds of compile time.

What stays the same:

- Exit codes: 0 success, 1 a reported failure, 2 a usage error.
- Every form that plugin v1.2.2 and the F8 binding run: `speak`,
  `speak --voice NAME TEXT`, `version`, `config get --json`,
  `config voices --json`, and `config set voice|speed VALUE`.
- `version` prints the bare version for the plugin. `--version` now prints
  `omatalk VERSION`, as clap does.
- `speak -5 degrees` is still text, and `--` still ends the flags.

What changed: `config set` with an unknown key is a usage error (exit 2) that
lists `voice` and `speed`, not a reported failure (exit 1). An unknown
single-dash word such as `speak -x` is now a usage error. Write `speak -- -x`.
