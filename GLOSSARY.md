# Omatalk

A local text-to-speech tool for Omarchy: press a hotkey and the machine speaks
the text you have selected.

## Language

**Selection**:
The text currently highlighted in the focused application, captured from the
Wayland primary selection.
_Avoid_: highlighted text, active text

**Source**:
Where spoken text comes from. Ordered: text given to `omatalk speak`, then the
Selection, then the Clipboard when the Selection is empty.
_Avoid_: input, buffer

**Clipboard**:
The Wayland clipboard contents, used only when the Selection is empty and
nothing is playing.

**Utterance**:
One unit of speech: text with its Voice, Speed, and Delivery fixed at the
start. A hotkey press resolves a Source to text. A new Utterance replaces the
currently playing one.
_Avoid_: request, job, playback

**Voice**:
Which voice pack an Utterance is spoken in. Bound when the Utterance starts,
from the config file or a one-shot override. A Render takes the configured one
or `--voice`.
_Avoid_: speaker, persona

**Speed**:
How fast an Utterance is spoken. Bound when the Utterance starts, from the
config file. A Render ignores that value and uses 1.0 unless given `--speed`.
_Avoid_: rate (that's the audio sample rate)

**Interrupt**:
Pressing the hotkey while speech is playing: the playing Utterance is cut off
immediately. If the press carries a new Source (different Selection), the new
Utterance begins. If the Selection is unchanged or empty, speech stops.
_Avoid_: cancel (for stop, which implies speech ends without a new Utterance)

**Daemon**:
The always-running local process that holds the TTS model warm, receives
Utterance requests, and plays audio. Starts at login and reads the config file
at each Utterance start.
_Avoid_: server, service (the systemd unit wraps the Daemon but is not the term)

**Delivery**:
How an Utterance trades latency for quality. Live is a Stream. Voiceover is a
Render.

**Stream**:
Speaking an Utterance batch by batch, starting playback before all audio is
synthesized. The first batch is a few words, so first audio comes fast. Later
batches grow up to Kokoro's 510-phoneme limit (the ramp), and each one is
synthesized while the previous one plays.

**Render**:
A whole script synthesized to a WAV file by the CLI, without the Daemon. It
has no Stream and no Interrupt: it favors quality over latency, packs each
paragraph on its own, and writes the file only when all of it succeeds.
_Avoid_: export, record

**Wake lead**:
Silence played before speech when the default sink is a suspended Bluetooth or
HDMI sink, so the device can unmute. Set by `wake_lead_ms`.

The Omarchy bar widget (megaphone, setup and config panel) lives in
https://github.com/zerobearing2/omarchy-omatalk-plugin, released on its own.
See `docs/adr/0004-plugin-own-repo.md`. Installed means `~/.local/bin/omatalk`
exists. Do not use `command -v omatalk` as the only probe.

## Bar states

- Not installed: normal color, tooltip "Omatalk is not installed". No socket
  follow, no unavailable.
- Installed and follow connected: idle or speaking.
- Installed and follow failed past a three-second grace, or Daemon `error`:
  unavailable (urgent).

## Panel states

- Not installed: setup. It links to the site install page, and the user runs
  the install command in a terminal. Plugin version comes from the plugin's
  `manifest.json`. No `omatalk config`, `version`, or preview `speak`.
- Installed: voice, speed, plugin version, and Daemon version
  (`omatalk version`). Config CLI uses `~/.local/bin/omatalk`.
