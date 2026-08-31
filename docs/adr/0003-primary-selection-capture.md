# Primary selection capture with clipboard fallback, no keystroke injection

The obvious way to grab "whatever is selected" on Linux is to simulate Ctrl+C
and read the clipboard (ydotool/wtype + wl-paste). We deliberately do not do
that. Omatalk reads the **Wayland primary selection** (`wl-paste --primary`)
directly: no synthetic keystrokes, no clipboard mutation, no race with the
user's real clipboard contents, no dependency on keyboard-injection daemons.
Apps that don't maintain a primary selection (some Electron apps) are covered
by falling back to the existing clipboard when the primary selection is
empty — the user can always copy explicitly and press the Speak Key.

Consequences: selection capture is read-only and side-effect-free, but in
non-cooperating apps the user needs one extra keystroke (copy) before
speaking. Acceptable trade-off vs. injecting keystrokes that can leak into
the focused app.
