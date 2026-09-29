# Speak

Speak turns text into audio. The text comes from the command, or from the selection when the command has no text, or from the clipboard when the selection is empty.

## Sub-features

- `speak-text` speaks the words given on the command line.
- `speak-voice` speaks one utterance in another voice and leaves the saved voice unchanged.
- `speak-selection` speaks the selection when the command has no text.
- `speak-clipboard` speaks the clipboard when the selection is empty.
- `speak-empty` plays nothing and reports that there is nothing to read when both sources are empty.

## How to get to it (user POV)

- Run `omatalk speak "text here"`.
- Run `omatalk speak --voice af_bella "text here"`.
- Press F8, which runs `omatalk speak` with no text.
- Run `omatalk speak` with no text in a terminal.

## Driving it with verify

Preconditions:

- `verify doctor "$RUN_ID"` prints `status: idle`.
- The saved voice is `af_heart`.
- The selection file contains `The selection says hello from omatalk.`
- The clipboard file contains `The clipboard says hello from omatalk.`

- **Speak text.** Run `verify drive-speak "$RUN_ID" -- "Hello from the omatalk verification run. This second sentence keeps the utterance speaking long enough to observe."` The command prints a drive directory. `result.txt` there is `ok`, `stdout.txt` is `ok`, `status.txt` contains `speaking` and a later `idle`, and `pcm.txt` lists a file marked `audio` whose args include `--rate 24000 --channels 1 -`.
- **One-shot voice.** Run `verify drive-speak "$RUN_ID" -- --voice af_bella "Hello from af_bella."` Then run `verify run "$RUN_ID" -- config get`. The drive directory's `result.txt` is `ok`, and `config get` still prints `voice = af_heart`.
- **Selection.** Run `verify drive-speak "$RUN_ID"`. The drive directory's `result.txt` is `ok`. The selection file is unchanged.
- **Clipboard fallback.** Empty the selection file, then run `verify drive-speak "$RUN_ID"`. Restore the selection sentence afterward. The drive directory's `result.txt` is `ok`, and the notify log does not contain `nothing to read`.
- **Nothing to read.** Note the PCM directory listing. Empty both the selection file and the clipboard file, then run `verify run "$RUN_ID" -- speak`. Restore both sentences afterward. Exit code is 0, stdout is `ok`, `status` stays `idle`, the notify log gains a line `nothing to read`, and the PCM directory listing is unchanged.

## Gotchas

- `ok` means the Daemon accepted the command. It does not mean audio finished, and it does not mean any samples were produced.
- `status` becomes `speaking` when the utterance starts. That can be earlier than the first nonzero sample.
- Lines of `idle` recorded before `speaking` are the previous state. The proof needs `idle` after `speaking`.
- The harness sets `sink_probe = ["false"]`, so no wake lead of zeros precedes speech in the PCM file. With the real `pactl`, a suspended Bluetooth or HDMI default sink gets `wake_lead_ms` of zeros first.
- `drive-speak` clears this run's PCM directory before it starts. Copy the drive directory if you still need the previous utterance.
- `speak-empty` must use `verify run`, not `drive-speak`. `drive-speak` requires audio and will fail when there is nothing to read.
- An unknown voice such as `not_a_real_voice` fails in the CLI with stderr `not_a_real_voice: not a known voice` and exit 1. The Daemon is not contacted.
