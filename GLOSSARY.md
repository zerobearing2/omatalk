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

**Package**:
`omatalk-bin`, the Omarchy package that is the only install. pacman owns
`/usr/bin/omatalk` and the systemd user unit. Installed with
`omarchy pkg add omatalk-bin`, upgraded by `omarchy update`.
_Avoid_: installer (that was the curl script), release (a tag and its tarball)

**Setup**:
`omatalk setup`: the per-user work the Package cannot do. It fetches the
models, enables the unit, asks before it binds F8, and links the bar plugin.
Every step skips itself when done, so it is safe to rerun.
_Avoid_: install (the package install is `omarchy pkg add`)

**Wake lead**:
Silence played before speech when the default sink is a suspended Bluetooth or
HDMI sink, so the device can unmute. Set by `wake_lead_ms`.

The Omarchy bar widget (megaphone and config panel) lives in
`plugin/zerobearing.omatalk/` and ships inside the Package at
`/usr/share/omatalk/omarchy-plugin/zerobearing.omatalk/`. Setup links it into
`~/.config/omarchy/plugins/`. It calls `/usr/bin/omatalk` only. See
`docs/adr/0009-the-omarchy-package-is-the-only-install.md`.

## Bar states

- Follow connected: idle or speaking.
- Follow failed past a three-second grace, or Daemon `error`: unavailable
  (urgent).

## Panel states

Voice, speed, media pause, wake lead, and the Daemon version (`omatalk
version`). The plugin ships with the Daemon, so there is one version. Config CLI
uses `/usr/bin/omatalk`. While the bar is unavailable, the panel notes that
changes apply once the Daemon starts.
