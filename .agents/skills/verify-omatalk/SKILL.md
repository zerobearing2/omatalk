---
name: verify-omatalk
description: >
  Drive the Omatalk CLI against an isolated Daemon and prove speak, stop,
  status, and config from the audio and replies a user would see. Use when
  validating a change to this repo, when the user says verify omatalk, or
  before claiming speak, stop, status, or config works.
---

# Verify Omatalk

Omatalk is a local CLI plus a Daemon on a Unix socket. The bar plugin is a separate repo and this skill does not drive it. The static site under `public/` is not part of a speech proof.

The harness is `.agents/skills/verify-omatalk/scripts/verify`. It builds `target/release/omatalk` when that binary is missing or older than `src/`, then starts a Daemon whose socket, config, and player are private to the run. It never uses the user socket at `${XDG_RUNTIME_DIR:-/run/user/$UID}/omatalk/omatalk.sock`, and it never runs `upgrade` or `uninstall`.

Each run id names one directory, `/tmp/omatalk-verify/<run-id>/`. Instance state is `<run-id>/instance/`. Proof is `<run-id>/evidence/` and is not deleted by cleanup. Two runs need two ids. Do not share a directory.

`OMATALK_VERIFY_MODELS` overrides the model directory. The default is `~/.local/share/omatalk/models`, read only. The directory must contain `kokoro-v1.0.onnx` and `voices-v1.0.bin`. The Daemon also needs `libonnxruntime.so.1` and `libespeak-ng.so.1` on the loader path, or `ORT_DYLIB_PATH` for a different onnxruntime build.

The script unsets `OMATALK_TEST_FAKE_ENGINE`. That flag synthesizes silence and still returns `ok`. A proof that only checks `ok` does not show speech.

## Launch

From the repo root:

```sh
chmod +x .agents/skills/verify-omatalk/scripts/verify
RUN_ID=$(.agents/skills/verify-omatalk/scripts/verify launch)
```

`launch` prints one line, the run id. The Daemon is ready when `doctor` prints `status: idle`. Readiness is that reply, plus a `model warm` line in the daemon log. The Daemon writes that line to unbuffered stderr, so it lands in the log file as soon as the model loads. Startup waits up to 90 seconds for the model load.

`launch` exits non-zero if the binary, models, or libraries are missing, or if the Daemon does not reach `idle`. On that failure it still runs cleanup for the run it started.

## Doctor

Run this after launch, and again whenever a drive looks wrong, before the next drive.

```sh
.agents/skills/verify-omatalk/scripts/verify doctor "$RUN_ID"
```

Exit 0 means the recorded pid is alive, the socket is inside this run's instance directory, `omatalk version` matches `Cargo.toml`, the model files and libraries are present, the log contains `model warm`, and `status` is `idle` or `speaking`. Anything else exits 1 and is not worth driving. Do not point a command at the user Daemon to work around a failed doctor.

## Drive

`run` executes one CLI invocation with the run's socket, config, and models, and appends the command, exit code, stdout, and stderr to `evidence/transcript.txt`.

```sh
.agents/skills/verify-omatalk/scripts/verify run "$RUN_ID" -- status
.agents/skills/verify-omatalk/scripts/verify run "$RUN_ID" -- config get
.agents/skills/verify-omatalk/scripts/verify run "$RUN_ID" -- speak -- "Hello from omatalk."
```

`drive-speak` is the speech proof. It records `status` across the utterance, requires stdout `ok`, requires a `speaking` sample and a later `idle` sample, and requires PCM with a nonzero sample.

```sh
.agents/skills/verify-omatalk/scripts/verify drive-speak "$RUN_ID" -- "Hello from the omatalk verification run. This second sentence keeps the utterance speaking long enough to observe."
```

`drive-stop` speaks, waits until `speaking`, sends `stop`, and requires a later `idle` plus stdout `ok`.

```sh
.agents/skills/verify-omatalk/scripts/verify drive-stop "$RUN_ID"
```

Both commands print the evidence directory they wrote under `evidence/drives/`.

Speak with no text reads the selection file, then the clipboard file. `doctor` prints both paths. The command does not call `wl-paste`. The player is the harness `record-pcm` command, not `pw-cat`, so the machine's speakers stay quiet. The Daemon appends `--rate 24000 --channels 1 -` and writes raw s16le mono on stdin. `record-notify` appends the notification text instead of calling `notify-send`.

The feature map is `features/README.md`. A proof that uses one entry point is incomplete when that file lists others.

## Evidence

A speech proof has all of these.

- The CLI command, exit code, stdout, and stderr.
- A `status` sample of `speaking` and a later sample of `idle`. `speak` prints `ok` when the utterance starts, not when audio ends. `idle` from before the utterance does not count.
- A PCM file that is not all zeros. The harness config sets `sink_probe = ["false"]`, so the Daemon never adds the wake lead of silence, and the file is the utterance alone.
- The player argument line containing `--rate 24000 --channels 1 -`.

`drive-speak` writes that set under the directory it prints. `result.txt` in that directory is `ok` only when every check passed. Copy nothing by hand. The script writes the files.

Config and status proofs are the CLI transcript plus the on-disk config when the command writes it. Read `evidence/transcript.txt` and the config path from `doctor`.

Do not treat `OMATALK_TEST_FAKE_ENGINE` as a successful speech proof. The PCM check rejects its silence.

## Cleanup

```sh
.agents/skills/verify-omatalk/scripts/verify cleanup "$RUN_ID"
```

Cleanup sends `SIGTERM` to the Daemon pid `launch` recorded, then `SIGKILL` if that pid is still alive after five seconds. It does not search by process name. It copies the daemon log and any leftover PCM into `evidence/`, then deletes `instance/`. The evidence directory stays. Confirm the drive directory is still there after cleanup before calling the proof done.

A failed launch or a failed drive should be cleaned the same way before the next attempt.

## Helpers

The executable is `.agents/skills/verify-omatalk/scripts/verify`.

```sh
verify launch
verify doctor RUN
verify run RUN -- ARG...
verify drive-speak RUN [-- SPEAK-ARG...]
verify drive-stop RUN [-- SPEAK-ARG...]
verify await-status RUN WORD SECONDS
verify pcm RUN
verify cleanup RUN
```

`record-pcm`, `record-notify`, `capture-primary`, and `capture-clipboard` are the Daemon's configured commands. Do not invoke them as the user path.
