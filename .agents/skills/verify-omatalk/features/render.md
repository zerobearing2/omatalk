# Render

Render writes a whole script to a WAV file with the real model. It does not use the Daemon, so nothing plays and `status` does not change.

## Sub-features

- `render-args` renders text given as arguments.
- `render-stdin` renders a script from stdin.
- `render-options` applies `--voice` and `--speed`.
- `render-daemon-down` works with no Daemon listening.
- `render-refusals` refuses a non-WAV path, empty text, and a missing `--out`.

## How to get to it (user POV)

- Run `omatalk render --out PATH [--voice NAME] [--speed N] [TEXT...]`.
- Run `omatalk render --out PATH < script.txt`.

## Driving it with verify

Preconditions:

- `verify doctor "$RUN_ID"` prints `status: idle`.
- Write output under the run's `evidence/` directory. Never write over a file this run did not create.

- **Arguments.** Run `verify drive-render "$RUN_ID"`. `result.txt` in the printed directory is `ok`, `out.wav` is kept there, `wav.txt` gives its size, and `status.txt` holds `idle` samples and no `speaking` sample.
- **Options.** Run `verify drive-render "$RUN_ID" -- --voice bm_george --speed 1.3 "Hello there, world."` `result.txt` is `ok`.
- **Stdin.** Take `binary`, `models`, `config`, and `evidence` from `doctor`. Run `printf 'One. Two.\n\nThree.\n' | OMATALK_MODELS="$MODELS" OMATALK_CONFIG="$CONFIG" "$BINARY" render --out "$EVIDENCE/stdin.wav"`. Exit code is 0, stdout names the path and a duration, and the audio is longer than the same text with no blank line.
- **Daemon down.** Run the same command with `OMATALK_SOCKET` set to `$INSTANCE/missing.sock`. Take `INSTANCE` from the socket path by removing `/omatalk.sock`. Exit code is 0 and the WAV is written.
- **Refusals.** Run `verify run "$RUN_ID" -- render --out x.mp3 hi`. Exit code is 1, stderr names ffmpeg, and no file appears. Run `verify run "$RUN_ID" -- render --out x.wav "   "`. Exit code is 1, stderr is `nothing to read`, and no file appears. Run `verify run "$RUN_ID" -- render hi`. Exit code is 2 and stderr is the usage line.

## Gotchas

- Render loads its own model, about 1 GiB, beside the Daemon. Each `drive-render` renders twice, so a long script takes twice as long.
- The default speed is 1.0, not the config speed.
- `--out` takes a path relative to the caller. The drive passes an absolute one.
- Pauses are heard, not proved by this drive. Paragraph breaks measure about 1 second of silence and sentence joints about 550 ms.
