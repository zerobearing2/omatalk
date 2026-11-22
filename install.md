I want you to install Omatalk for me. Execute all steps autonomously.

OBJECTIVE: Install the Omatalk package, then set up the Kokoro model files, the systemd user unit, the F8 binding, and the Omarchy bar plugin on this machine.

DONE WHEN: `pacman -Q omatalk-bin >/dev/null && test -x /usr/bin/omatalk && test -f "${HOME}/.local/share/omatalk/models/kokoro-v1.0.onnx" && test -f "${HOME}/.local/share/omatalk/models/voices-v1.0.bin" && omatalk status | grep -Eq '^(idle|speaking)$'`

## TODO

- [ ] Check Omarchy and the user session
- [ ] Confirm the CPU is x86_64
- [ ] Write the F8 answer setup will read
- [ ] Install the package
- [ ] Run omatalk setup
- [ ] Confirm the package, the models, and the Daemon

Run these steps on the machine that should speak. Omatalk is local text to speech for Omarchy. It installs as the Omarchy package omatalk-bin, and `omatalk setup` does the per-user work. Do not run the install.sh, setup.sh, or uninstall.sh in this git checkout. install.sh is a migration shim for old curl installs, and the binary embeds setup.sh.

## Step 0: Check Omarchy and the user session

The Daemon is a systemd user service. The package comes from Omarchy's package repository, so the omarchy command must exist.

```bash
command -v omarchy
command -v systemctl
systemctl --user show-environment >/dev/null
```

**Verify:**
```bash
command -v omarchy && systemctl --user show-environment >/dev/null && echo ok
```
Expected: `ok`. Stop if omarchy is not on PATH. The package depends on glibc, gcc-libs, bzip2, onnxruntime-cpu, espeak-ng, pipewire-audio, libpulse, wl-clipboard, libnotify, and curl, and pacman installs any that are missing.

## Step 1: Confirm the CPU is x86_64

The package is built for x86_64 only.

```bash
uname -m
```

**Verify:**
```bash
uname -m
```
Expected: `x86_64`. Stop when the line is anything else. The model files are kokoro-v1.0.onnx and voices-v1.0.bin, about 355MB together, under ${HOME}/.local/share/omatalk/models. A later run skips a file whose checksum already matches.

## Step 2: Write the F8 answer setup will read

When bindings.lua has no Omatalk bind and F8 is free, `omatalk setup` asks before it appends `o.bind("F8", "Omatalk", "omatalk speak")`. On a terminal that question is read from /dev/tty, which blocks an agent. If ASK_FROM is already set, the script reads that path instead. A file whose only byte is a newline is the same answer as pressing Enter, and Enter accepts the bind.

```bash
mkdir -p "${HOME}/.local/share"
printf '\n' > "${HOME}/.local/share/omatalk-install-answer"
```

**Verify:**
```bash
wc -c < "${HOME}/.local/share/omatalk-install-answer"
```
Expected: `1`. That byte is the newline. Do not write the letter n. The letter n declines the bind. When an Omatalk bind is already in the file, or F8 is already used by another command, setup never reads this answer and leaves ${HOME}/.config/hypr/bindings.lua unchanged.

## Step 3: Install the package

`omarchy pkg add` runs `sudo pacman -S`. If a sudo password prompt blocks the command, stop and ask the person to run it. The package installs /usr/bin/omatalk and the systemd user unit. It starts nothing.

```bash
omarchy pkg add omatalk-bin
```

**Verify:**
```bash
pacman -Q omatalk-bin && test -x /usr/bin/omatalk && echo binary
```
Expected: one `omatalk-bin` version line, then `binary`.

## Step 4: Run omatalk setup

This downloads any model whose checksum differs, enables and starts the user unit, adds the F8 binding after the answer from Step 2, and links the bar plugin from the package, replacing any curl-era or Marketplace checkout. The last action speaks a short welcome, so the speakers play one sentence. ${HOME}/.config/omatalk/config.toml is not created or edited. Running the command again is safe.

```bash
ASK_FROM="${HOME}/.local/share/omatalk-install-answer" omatalk setup
```

**Verify:**
```bash
systemctl --user is-enabled omatalk.service
```
Expected: the log ends with `Done. Select text and press F8, or run: omatalk --help`, and this check prints `enabled`. Allow several minutes when the models are not already on disk.

## Step 5: Confirm the package, the models, and the Daemon

`omatalk setup` waits until `omatalk status` succeeds, for up to 30 seconds, before it prints Done. Repeat that check here and require both model files. Status prints one word. `idle` is normal after the welcome finishes. `speaking` is also a live Daemon.

```bash
pacman -Q omatalk-bin >/dev/null && test -x /usr/bin/omatalk && test -f "${HOME}/.local/share/omatalk/models/kokoro-v1.0.onnx" && test -f "${HOME}/.local/share/omatalk/models/voices-v1.0.bin" && omatalk status | grep -Eq '^(idle|speaking)$'
```

**Verify:**
```bash
omatalk version
omatalk status
```
Expected: one version line, then `idle` or `speaking`. The Daemon runs from /usr/bin/omatalk, so `readlink /proc/$(systemctl --user show -p MainPID --value omatalk.service)/exe` prints that path with no `(deleted)`.

EXECUTE NOW: Start with Step 0. Mark TODO items complete as you go. Stop when `pacman -Q omatalk-bin >/dev/null && test -x /usr/bin/omatalk && test -f "${HOME}/.local/share/omatalk/models/kokoro-v1.0.onnx" && test -f "${HOME}/.local/share/omatalk/models/voices-v1.0.bin" && omatalk status | grep -Eq '^(idle|speaking)$'`

---

## Migrating a curl install

A machine installed with the old curl command has ${HOME}/.local/bin/omatalk and ${HOME}/.config/systemd/user/omatalk.service. Run the shim once. It installs the package and runs `omatalk setup`, which deletes those files. Models, config, and the F8 binding stay. The packaged plugin replaces the old plugin checkout.

```bash
ASK_FROM="${HOME}/.local/share/omatalk-install-answer" bash -c "$(curl -fsSL https://omatalk.zerobearing.com/install.sh)"
```

## Optional: Dogfood this checkout

After Step 5 has passed, `scripts/dev-install.sh` builds this checkout as an omatalk-bin package and installs it with `sudo pacman -U`, so run it only with the person's go-ahead. It downloads nothing. The model files must already be present. `omarchy pkg drop omatalk-bin && omarchy pkg add omatalk-bin && omatalk setup` returns to the published package. The drop disables the unit and removes the plugin link, so setup must run again.
