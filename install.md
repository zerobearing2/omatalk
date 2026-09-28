I want you to install Omatalk for me. Execute all steps autonomously.

OBJECTIVE: Install the Omatalk Daemon, the Kokoro model files, the systemd user unit, and the Omarchy bar plugin on this machine.

DONE WHEN: `test -x "${HOME}/.local/bin/omatalk" && test -f "${HOME}/.local/share/omatalk/models/kokoro-v1.0.onnx" && test -f "${HOME}/.local/share/omatalk/models/voices-v1.0.bin" && "${HOME}/.local/bin/omatalk" status | grep -Eq '^(idle|speaking)$'`

## TODO

- [ ] Check Omarchy and the user session
- [ ] Confirm the CPU is x86_64
- [ ] Write the F8 answer the installer will read
- [ ] Run the release installer
- [ ] Confirm the binary, the models, and the Daemon

Run these steps on the machine that should speak. Omatalk is local text to speech for Omarchy. Use https://omatalk.zerobearing.com/install.sh. That URL downloads the install.sh shipped with the latest GitHub release and runs it, so the script and the binary match. Do not run the install.sh in this git checkout. That copy gets a real release tag and checksum only when a release is published.

## Step 0: Check Omarchy and the user session

The Daemon is a systemd user service. The installer exits when the omarchy command is missing. It adds packages only with omarchy pkg add.

```bash
command -v omarchy
command -v systemctl
systemctl --user show-environment >/dev/null
test -n "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
```

**Verify:**
```bash
command -v omarchy && systemctl --user show-environment >/dev/null && echo ok
```
Expected: `ok`. Packages the installer may add are curl, pipewire, wl-clipboard, onnxruntime-cpu, espeak-ng, and libnotify. Stop if omarchy is not on PATH.

## Step 1: Confirm the CPU is x86_64

The published artifact is omatalk-x86_64.tar.gz. The release script checks its SHA-256. No other architecture is published.

```bash
uname -m
```

**Verify:**
```bash
uname -m
```
Expected: `x86_64`. Stop when the line is anything else. The model files are kokoro-v1.0.onnx and voices-v1.0.bin, about 355MB together, under ${HOME}/.local/share/omatalk/models. A later run skips a file whose checksum already matches.

## Step 2: Write the F8 answer the installer will read

When bindings.lua has no Omatalk bind and F8 is free, the installer asks before appending `o.bind("F8", "Omatalk", "omatalk speak")`. On a terminal that question is read from /dev/tty, which blocks an agent. If ASK_FROM is already set, the script reads that path instead. A file whose only byte is a newline is the same answer as pressing Enter, and Enter accepts the bind.

```bash
mkdir -p "${HOME}/.local/share"
printf '\n' > "${HOME}/.local/share/omatalk-install-answer"
```

**Verify:**
```bash
wc -c < "${HOME}/.local/share/omatalk-install-answer"
```
Expected: `1`. That byte is the newline. Do not write the letter n. The letter n declines the bind. When an Omatalk bind is already in the file, or F8 is already used by another command, the installer never reads this answer and leaves ${HOME}/.config/hypr/bindings.lua unchanged.

## Step 3: Run the release installer

This downloads the pinned release, checks its digest, fetches any model whose checksum differs, stops omatalk.service, installs ${HOME}/.local/bin/omatalk, enables the user unit, and adds the bar plugin when that git checkout is missing. The last action speaks a short welcome, so the speakers play one sentence. ${HOME}/.config/omatalk/config.toml is not created or edited. Running the command again is safe.

```bash
ASK_FROM="${HOME}/.local/share/omatalk-install-answer" bash -c "$(curl -fsSL https://omatalk.zerobearing.com/install.sh)"
```

**Verify:**
```bash
test -x "${HOME}/.local/bin/omatalk" && echo binary
```
Expected: the log ends with `Done. Select text and press F8, or run: omatalk speak|stop|status|upgrade|uninstall`, and this check prints `binary`. Allow several minutes when the models are not already on disk.

## Step 4: Confirm the binary, the models, and the Daemon

The installer waits until `omatalk status` succeeds, for up to 30 seconds, before it prints Done. Repeat that check here and require both model files. Status prints one word. `idle` is normal after the welcome finishes. `speaking` is also a live Daemon.

```bash
test -x "${HOME}/.local/bin/omatalk" && test -f "${HOME}/.local/share/omatalk/models/kokoro-v1.0.onnx" && test -f "${HOME}/.local/share/omatalk/models/voices-v1.0.bin" && "${HOME}/.local/bin/omatalk" status | grep -Eq '^(idle|speaking)$'
```

**Verify:**
```bash
"${HOME}/.local/bin/omatalk" version
"${HOME}/.local/bin/omatalk" status
```
Expected: one version line, then `idle` or `speaking`. The launcher is the file ${HOME}/.local/bin/omatalk. Do not treat `command -v omatalk` as proof.

EXECUTE NOW: Start with Step 0. Mark TODO items complete as you go. Stop when `test -x "${HOME}/.local/bin/omatalk" && test -f "${HOME}/.local/share/omatalk/models/kokoro-v1.0.onnx" && test -f "${HOME}/.local/share/omatalk/models/voices-v1.0.bin" && "${HOME}/.local/bin/omatalk" status | grep -Eq '^(idle|speaking)$'`

---

## Optional: Dogfood this checkout

After Step 4 has passed, `scripts/dev-install.sh` builds this checkout, replaces ${HOME}/.local/bin/omatalk, installs this tree's unit, and restarts the Daemon. It downloads nothing. The model files must already be present. `omatalk upgrade` returns to the published binary.
