# Status

Status tells a user whether the Daemon is idle, speaking, or in error, without changing the utterance.

## Sub-features

- `status-idle` prints `idle` when nothing is playing.
- `status-speaking` prints `speaking` during an utterance.
- `status-down` prints an error when no Daemon is listening, and does not notify.

## How to get to it (user POV)

- Run `omatalk status`.

## Driving it with verify

Preconditions:

- `verify doctor "$RUN_ID"` prints `status: idle` for the idle check.
- The down check uses a socket path that does not exist inside the same instance directory. Do not use the user socket.

- **Idle.** Run `verify run "$RUN_ID" -- status`. Exit code is 0 and stdout is `idle`.
- **Speaking.** Run `verify drive-speak "$RUN_ID" -- "Hello from the omatalk verification run. This second sentence keeps the utterance speaking long enough to observe."` The drive directory's `status.txt` contains a `speaking` line. That line is this feature's proof.
- **Daemon down.** Run the binary with `OMATALK_SOCKET` set to `$INSTANCE/missing.sock` and `OMATALK_CONFIG` set to the config path `doctor` prints. Take `INSTANCE` from the socket path by removing `/omatalk.sock`. The command is `omatalk status`. Exit code is 1, stderr is `daemon not running`, and the notify log is unchanged.

## Gotchas

- `status` does not send a notification when the Daemon is down. `speak` and `stop` do. A new notify line means the wrong command was run.
- `error` is a real Daemon state after a failed utterance. `doctor` rejects it. Read `evidence/notify.log` and the daemon log, then `cleanup` and `launch` again before the next drive.
- `await-status` can miss a short utterance if it starts after `speak` has already returned to idle. Use `drive-speak` for the speaking sample.
