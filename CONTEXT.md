# Omatalk

A local text-to-speech tool for Omarchy: press a hotkey and the machine speaks
the text you have selected. The reverse of dictation — instead of you talking
to the machine, the machine reads back to you.

## Language

**Selection**:
The text currently highlighted in the focused application, captured via the
Wayland primary selection.
_Avoid_: highlighted text, active text

**Source**:
Where spoken text comes from. Ordered: the Selection first, the Clipboard as
fallback when the Selection is empty.
_Avoid_: input, buffer

**Clipboard**:
The Wayland clipboard contents, used only when the Selection is empty.

**Utterance**:
One unit of speech requested by a hotkey press: a Source resolved to text,
synthesized and played. A new Utterance replaces the currently playing one.
_Avoid_: request, job, playback

**Interrupt**:
Pressing the hotkey while speech is playing: the playing Utterance is cut off
immediately. If the press carries a new Source (different Selection), the new
Utterance begins; if the Selection is unchanged or empty, speech simply stops.
_Avoid_: cancel (for stop, which implies speech ends without a new Utterance)

**Daemon**:
The always-running local process that holds the TTS model warm, receives
Utterance requests, and plays audio. Starts at login.
_Avoid_: server, service (the systemd unit wraps the Daemon but is not the term)

**Stream**:
Speaking a Utterance sentence-by-sentence, starting playback before all audio
is synthesized.
