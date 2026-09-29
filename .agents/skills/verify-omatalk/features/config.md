# Config

Config shows the saved voice and speed, changes them, and lists the voices the model directory contains. The next utterance uses the file on disk. An utterance that has already started keeps the voice and speed it started with.

## Sub-features

- `config-get` prints the effective settings.
- `config-set-voice` saves a known voice.
- `config-set-speed` saves a speed from 0.5 through 2.0.
- `config-voices` lists the voice names.
- `config-reject` refuses an unknown voice and a speed outside the range, and leaves the file unchanged.

## How to get to it (user POV)

- Run `omatalk config get`.
- Run `omatalk config get --json`.
- Run `omatalk config set voice af_bella`.
- Run `omatalk config set speed 1.25`.
- Run `omatalk config voices`.
- Run `omatalk config voices --json`.

## Driving it with verify

Preconditions:

- `verify doctor "$RUN_ID"` prints `status: idle`.
- The config file still has `voice = af_heart` and `speed = 1.0`. No Daemon is required for these commands, but use the run's config and models so the user file is not modified.

- **Read settings.** Run `verify run "$RUN_ID" -- config get`. Exit code is 0 and stdout contains `voice = af_heart` and `speed = 1.0`.
- **Read settings as JSON.** Run `verify run "$RUN_ID" -- config get --json`. Exit code is 0 and the single stdout line is a JSON object whose `voice` is `af_heart` and whose `speed` is `1.0`.
- **Save a voice.** Run `verify run "$RUN_ID" -- config set voice af_bella`. Exit code is 0 and stdout is empty. The config file `doctor` prints now contains `voice = "af_bella"`. Set it back with `verify run "$RUN_ID" -- config set voice af_heart` and read the file again.
- **Save a speed.** Run `verify run "$RUN_ID" -- config set speed 1.25`. The config file contains `speed = 1.25`. Set it back with `verify run "$RUN_ID" -- config set speed 1.0`.
- **List voices.** Run `verify run "$RUN_ID" -- config voices`. Exit code is 0, stdout is one name per line, and `af_heart` and `af_bella` are both present.
- **Reject a voice.** Run `verify run "$RUN_ID" -- config set voice not_a_real_voice`. Exit code is 1, stderr contains `not_a_real_voice`, and the config file still has `voice = "af_heart"`.
- **Reject a speed.** Run `verify run "$RUN_ID" -- config set speed 2.1`. Exit code is 1, stderr contains `0.5` and `2.0`, and the config file still has `speed = 1.0`.

## Gotchas

- `config set` prints nothing on success. The file is the proof.
- `config voices` reads the model directory. It does not need a running Daemon. A missing `voices-v1.0.bin` fails the command.
- Voice names look like `af_heart`. `not_a_real_voice` is rejected. A name that is not two letters, an underscore, and a lowercase word is rejected too.
- Speed must be a number from 0.5 through 2.0. `fast` is rejected.
- These commands under `verify run` edit the run's config file. Running `omatalk config set` yourself, without the run's `OMATALK_CONFIG`, edits `~/.config/omatalk/config.toml`.
