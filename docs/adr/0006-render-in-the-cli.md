# Render runs in the CLI process, packed by paragraph

Extends ADR-0005, which says only `omatalk daemon` loads onnxruntime and
espeak-ng. Now `omatalk render` loads them too. `version`, `config`, `speak`,
and the rest still do not.

`omatalk render --out talk.wav` turns a whole script into a WAV file. It is
for voiceover, where nobody waits on first audio and the listener hears every
join between batches. Three decisions follow.

- **In the CLI process, not the Daemon.** A long render holds the synth thread
  for minutes, and the hotkey must never wait behind it. Running it in the
  Daemon would also mean a new protocol request, a progress channel, and a way
  to cancel. The CLI process needs none of that: no protocol change, and
  Ctrl-C ends it. The cost is a second copy of the model in memory while a
  render runs, and a render works with the Daemon stopped.
- **The ramp is unused.** The Stream starts with a 32-phoneme batch so first
  audio comes fast, then grows batches. A file has no first audio to
  hurry. Those small early batches only add joints, and the ramp cuts where
  the size cap falls, which can be mid-sentence. A Voiceover packs each
  paragraph on its own through the same `split_phonemes` the full-size
  Stream batches use, so a batch never crosses a paragraph, and cuts fall at
  sentence, then clause, then word bounds.
- **Pauses come from the script's structure.** Kokoro's own gaps between
  sentences inside one batch measure 480 to 670 ms, and the trimmed joint
  between two batches adds 110 to 150 ms. A sentence joint of 420 ms lands
  where the model would have put the gap. A clause joint is 170 ms, a word
  joint is 0, a paragraph is 900 ms, and the last batch has none. The pauses
  scale by 1/speed, because the model's own gaps shrink with speed. The
  Stream's fixed 250 and 100 ms pauses are unchanged. A blank line in the
  script is a beat, a single newline is not.

Measured on a 72 s script, three ways of cutting it into batches, all at about
5x real time:

| Batching | Batches | ms per phoneme | Notes |
|---|---|---|---|
| Stream ramp | 5 | 54.9 to 65.4 | 2 cuts mid-sentence |
| Pack by paragraph | 3 | 56.1 to 57.6 | the narrowest spread |
| One batch per sentence | 16 | 50.3 to 68.6 | short sentences 1 dB louder |

ms per phoneme is audio length per phoneme, so a wide range means the pace
changes from batch to batch. Packing keeps it within 1.5 ms, which is why a
Voiceover packs. Per-sentence batches vary most, and the short ones come out
louder. Pinning the style row to one index, so every batch gets the same
voice vector, was far worse, up to 106.7 ms per phoneme. The style row stays
indexed by each batch's token count, as the Stream does.

Considered and rejected:

- **Render inside the Daemon.** See above: it blocks the hotkey, and it needs
  protocol and cancel work for a file nobody plays live.
- **One batch per sentence.** More joints, uneven pace, and uneven loudness.
- **A pinned style row.** Measured worst.
- **MP3, FLAC, or other formats.** WAV needs no dependency. `ffmpeg` converts
  the file, and the command says so when given another extension.
- **Config speed as the default.** Speed in config is a listening preference.
  A render defaults to 1.0 and takes `--speed` for anything else.

Consequences: the binary gains one command and one module and no
dependencies. A render keeps all its audio in memory until the end, then
writes a temp file beside the target and renames it, so a failed render
leaves no file and an existing target untouched. The WAV is 16-bit mono at
24 kHz, so a script is limited to what RIFF's 32-bit sizes hold, about 24
hours.

## Memory

TODO(memory table)
