# Stop

Stop cuts off the utterance that is playing. The next status a user sees is idle.

## Sub-features

- `stop-speaking` stops an utterance after status has reached `speaking`.
- `stop-idle` answers `ok` when nothing is playing.

## How to get to it (user POV)

- Run `omatalk stop`.
- Press F8 again while the same selection is still speaking. That path is `omatalk speak` with no new text, and the map covers it as stop only when a selection fixture is set up to repeat the same text. Use `omatalk stop` for this feature.

## Driving it with verify

Preconditions:

- `verify doctor "$RUN_ID"` prints `status: idle`.

- **Stop while speaking.** Run `verify drive-stop "$RUN_ID"`. The command prints a drive directory. `stop-stdout.txt` is `ok`, `exit.txt` is `0`, and `status.txt` contains `speaking` with a later `idle`.
- **Stop while idle.** After the previous step has returned to idle, run `verify run "$RUN_ID" -- stop`. Exit code is 0, stdout is `ok`, and `verify run "$RUN_ID" -- status` prints `idle`.

## Gotchas

- `drive-stop` sends `stop` as soon as it sees `speaking`. A PCM file from this drive may be short. The status change is the proof, not a finished sentence of audio.
- `omatalk stop` while the Daemon is down prints `daemon not running` on stderr, exits 1, and appends a notify line. That is a down Daemon, not this feature. Cover it from the status map only if you point `OMATALK_SOCKET` at a missing socket inside the instance directory.
- Do not stop the user Daemon. `drive-stop` uses the run socket only.
