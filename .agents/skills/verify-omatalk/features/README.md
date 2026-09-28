# Omatalk verification map

This directory is the maintained source for verifying the user-facing behavior of Omatalk. Read the index before driving the app, then use the matching feature file as the recipe.

The harness is `.agents/skills/verify-omatalk/scripts/verify`. The bar plugin and the site are not features of this map.

## Baseline preconditions

- Run `verify launch` and keep the printed run id.
- Run `verify doctor RUN` and require exit 0 and `status: idle` before the first drive.
- The isolated config starts at voice `af_heart` and speed `1.0`.
- The selection file and the clipboard file hold the sentences `doctor` prints. Speak with no text reads those files. It does not read the Wayland selection or clipboard.
- The player records PCM. It does not play through the speakers.
- Never drive a Daemon this run did not start. Never run `omatalk upgrade` or `omatalk uninstall`.

## Driving conventions

- Start every recipe from `status: idle` unless its preconditions say otherwise.
- Pass the run id from `launch` to every later command.
- Treat every command as literal. Keep quoted text and flags unchanged.
- Run CLI actions through `verify run` or the `drive-speak` and `drive-stop` commands named in the feature file.
- After a recipe changes voice or speed, set them back to `af_heart` and `1.0` before the next recipe.
- Do not delete `evidence/` during cleanup.

## Proof and skip reporting

- Capture the command and the resulting state, not only the final `ok`.
- Speech proof includes stdout `ok`, a `speaking` sample, a later `idle` sample, and a PCM file larger than 2304 bytes that is not all zeros.
- CLI proof includes the command, stdout, stderr, and exit code in `evidence/transcript.txt`.
- A config change is proved by reading the config path `doctor` prints, not by the empty stdout of `config set`.
- Record the feature file and the entry point with every artifact. `drive-speak` and `drive-stop` do this by writing `evidence/drives/<stamp>/`.
- Report an unreachable path with the command that was run and the unmet precondition.
- Do not report a skipped entry point as verified through a different path.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then uses exactly four H2 sections in this order.

1. `Sub-features` lists short IDs with one line for each behavior.
2. `How to get to it (user POV)` lists every user entry point.
3. `Driving it with verify` starts with `Preconditions:` and uses labeled bullets that pair each user action with an exact command and observable result.
4. `Gotchas` lists traps that can waste or invalidate a verification run.

Keep implementation details out of the map. Name only user paths, stable handles, required state, commands, and observable proof.

## Features

- [Speak](./speak.md) covers text, a one-shot voice, the selection, the clipboard fallback, and an empty source.
- [Stop](./stop.md) covers cutting off an utterance that is already speaking.
- [Status](./status.md) covers idle, speaking, and a Daemon that is not running.
- [Config](./config.md) covers reading and writing voice and speed, and listing voices.
