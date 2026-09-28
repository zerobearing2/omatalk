# Omatalk 0.9: one binary, one actor

## Problem

Replace the Python Daemon and CLI (v0.5.1) with Rust at `0.9.0`. The frozen bar plugin v1.2.2, the Hyprland `omatalk speak` binding, the site, and existing installs must not notice anything except better speech and faster first audio. Everything listed under "External contracts" in the grounding stays: socket path and line protocol (`follow`/`status`/`stop` exact), CLI argv, stdout, stderr texts, and exit codes, config path and TOML keys, launcher file, unit name, notify texts, env vars, and release asset names.

Four things make the shape non-obvious. First, the prototype pipeline (normalize, spaCy tokenizer and tagger ports, misaki, Kokoro fp32 through `ort`) is proven but written as a batch CLI. It has to become a cancellable stream. Second, the ONNX run is the whole latency budget, at ~155 ms per call plus ~6 ms per phoneme. The first batch must be short, and later batches must grow no faster than playback can hide them. Third, libespeak-ng has process-global state and ORT sessions are large, so one thread must own synthesis. Fourth, Python's concurrency bugs came from shared mutable state guarded by locks and `cancel.is_set()` checks sprinkled through every step (see `daemon/player.py`, about 200 lines of lock choreography). The rewrite should make those races unrepresentable, not re-guard them.

## Usage (caller's view)

The external surface is unchanged. Hotkey, plugin, and installer run the same commands:

```sh
omatalk speak                       # Selection, else Clipboard (when idle); prints "ok"
omatalk speak --voice af_bella "Hi, I'm Bella. This is what I sound like."
omatalk speak -- --voice is text    # fixed: "--" ends options
omatalk stop | status | version | --version
omatalk config get --json           # {"capture_clipboard":[...],"lang":"en-us",...,"speed":1.0,"voice":"af_heart"}
omatalk config set speed 1          # silent rc 0; writes speed = 1.0, touches nothing else
omatalk config voices --json        # works with the Daemon stopped
omatalk upgrade | uninstall
omatalk daemon                      # new: the unit's ExecStart. Binds the socket only once the model is warm.
```

```sh
$ omatalk config set speed fast;        echo $?   # fast: speed must be a number        1
$ omatalk config set voice nope;        echo $?   # nope: not a known voice             1
$ omatalk config set lang en-gb;        echo $?   # lang: not settable via config set (edit config.toml directly)  1
$ omatalk status  # Daemon down       -> stderr "daemon not running", rc 1, no notify
$ omatalk speak   # Daemon down       -> same, plus notify "daemon not running — systemctl --user start omatalk"
$ omatalk frob                        -> stderr "usage: omatalk ...", rc 2
```

Wire protocol (one line per connection). `follow`, `stop`, and `status` stay byte-identical. `speak` changes because the CLI and Daemon ship together:

```
speak                                        # a press: Daemon resolves Selection/Clipboard
speak {"text":"line one\nline two","voice":"af_bella"}   # JSON payload; both fields optional
```

A JSON string never contains a raw newline, so multi-line text and `--voice`-looking text survive. There is no hand-rolled escaper.

Internal call sites (derived first, the types follow from them):

```rust
// daemon.rs, connection thread: parse at the boundary, never block the actor on I/O.
let press = match req.text.as_deref().and_then(Text::new) {
    Some(text) => Press::Inline(text),                    // blank inline text is a plain press, as in v0.5
    None => Press::Selection(Text::new(&exec::capture(&config.capture_primary))),
};
let outcome = match send(press) {                          // the Clipboard is read only when the actor asks
    Some(SpeakOutcome::NeedClipboard) => send(Press::Clipboard(Text::new(&exec::capture(&config.capture_clipboard)))),
    other => other,
};
if outcome == Some(SpeakOutcome::NothingToRead) {
    exec::notify(&config.notify, "nothing to read");     // before replying "ok"
}

// daemon.rs, actor thread: the whole Utterance lifecycle is one match on Phase.
match decide(self.phase.playing(), press) {
    Decision::Start(text) => self.phase = Phase::Speaking { text, stream: Stream::start(utterance, io, generation, &self.synth, on_end) },
    Decision::Stop        => self.phase = Phase::Idle,          // dropping the Stream interrupts it
    Decision::Keep        => {}                                 // a Clipboard follow-up that lost a race
    Decision::NeedClipboard => return SpeakOutcome::NeedClipboard,
    Decision::NothingToRead => return SpeakOutcome::NothingToRead,
}

// stream.rs, synth thread: the engine is a deep module. Text in, PCM out, cancellable.
for pcm in engine.speak(&utterance, &stop) { if stop.is_fired() || pcm_tx.send(pcm?).is_err() { break } }
```

## Shape

**One crate, one binary** (`src/main.rs` plus `src/lib.rs` so examples and tests reach modules). Dependencies: `ort` (`load-dynamic`), `libloading` (espeak-ng), `fancy-regex`, `unicode-normalization`, `serde_json`, `toml_edit`, `npyz` (npz), `libc` (SIGTERM, poll, O_NONBLOCK). No async runtime and no clap.

```
src/main.rs          ExitCode from cli::main()
src/cli.rs           Cli enum, parse(argv) -> Result<Cli, Usage>, run(), client side of the socket, upgrade/uninstall
src/protocol.rs      Request::parse/encode, reply words. The only file that spells wire strings.
src/config.rs        Paths::from_env (one place for every env var), Config (typed), Setting (config set), toml_edit writer
src/voices.rs        VoiceName, list(models) from npz entry names, no ORT
src/exec.rs          Argv, capture() with 2 s timeout, notify() synchronous with timeout
src/daemon/mod.rs    serve(): load ∥, bind, accept; connection(); speak(); ask()
src/daemon/actor.rs  Command; Actor; Phase; decide(); Followers
src/stream.rs        Stream (one Utterance in flight), Synth thread handle, playback thread, Player reaping
src/speech/mod.rs    Engine trait (a pull iterator of PCM batches), Utterance, Text, StopToken, load_engine()
src/speech/kokoro.rs ORT session, vocab, voice styles, trim, pause padding (prototype main.rs)
src/speech/batch.rs  phoneme split/pack plus the first-audio Ramp (pure)
src/speech/fake.rs   FakeEngine (OMATALK_TEST_FAKE_ENGINE), RecordingEngine for tests
src/speech/g2p/      normalize, tokenize, tagger, misaki, espeak: prototype ports moved as-is
data/                tokenizer.json, tagger.bin, us_gold.json, us_silver.json, vocab.json (embedded)
examples/parity.rs   the prototype's --parity-e2e, tokparity, tagparity as one lever
```

Hotkey to first audio reads in three files: `cli.rs` → `daemon.rs` → `stream.rs`. The engine sits behind one trait method.

**Core types** (per type-system-discipline and model-the-domain):

- `Text` is trimmed and non-empty, built only by `Text::new(&str) -> Option<Text>`. "Empty Source" is `None`, never `""`.
- `Utterance { text: Text, voice: VoiceName, speed: Speed }` is built once at Utterance start from a freshly loaded `Config`. It is immutable and moves into the synth job, so a running Utterance cannot see a later config edit. That is the binding contract, enforced by ownership rather than by rereading config.
- `Speed(f32)` is constructed only through `Speed::parse(raw)`, which accepts `1` and `1.0`. Its error `Display` impls are the exact stderr lines.
- There is no language binding. en-us is the only lexicon the misaki port has, so `lang` in config.toml is accepted and ignored (see Open questions).
- `Phase` is `Idle | Speaking { text: Text, stream: Stream } | Error`. `State` (`idle|speaking|error`) is derived by `phase.state()`, so there is no second field to keep in sync. "Speaking without a stream" and "stream without speaking" cannot be written.
- `Stream` owns the cancellation of one Utterance. `impl Drop for Stream` fires its `StopToken`. Replacing or clearing `Phase::Speaking` cuts playback, with no separate `stop_current()` to forget. Interrupt is `self.phase = Phase::Speaking { text, stream }`. The old Stream drops in the same assignment.
- `Gen(u64)` tags each Stream. The only asynchronous input to the actor is `StreamEnded { generation, outcome }`, and it is ignored unless `generation` matches the current Stream. That single comparison replaces the seven `cancel.is_set()` checks in `omatalkd._run`.
- `Espeak` is created at most once per process (guarded by a `static AtomicBool`) and is `Send + !Sync`. espeak-ng's global state can move to the synth thread but never be shared.

**Concurrency: one actor, structural single-writers** (per separate-before-serializing-shared-state):

| Thread | Owns | Talks to | Blocks on |
|---|---|---|---|
| main (accept) | `UnixListener` | spawns connection threads | `accept` |
| connection (one per request, short-lived) | one `UnixStream`, one `Config` snapshot | actor via `Sender<Command>` plus a one-shot reply | read with 2 s deadline, capture, notify |
| actor (1) | `Phase`, `Gen` counter, followers | Synth, Streams | `recv()` only. It never spawns a process or does blocking I/O. |
| synth (1) | `Box<dyn Engine>` (ORT session, G2P, espeak) | playback thread by bounded `SyncSender<Pcm>` | ORT run (terminable) |
| playback (one per Stream) | speech `Child`, its stdin fd | actor by `StreamEnded` | sink probe (≤150 ms), `poll` on stdin, 10 ms tick |

- **No lost transitions, by construction.** The actor handles `Follow(stream)` in one step. It writes the current state line and pushes the stream onto `followers`. No other command can run in between. Broadcast lives in exactly one place, the actor loop: `let before = state(); handle(cmd); if state() != before { broadcast }`. Interrupt is Speaking→Speaking, which is no change and no line, so followers never see `idle` between Utterances.
- **Slow or hung followers.** Follower sockets are non-blocking. A state line is at most 9 bytes. A `WouldBlock` means the client has left the whole socket buffer unread (~2.5 KB, about 278 lines, measured on a default socketpair, since per-write overhead dominates), so it is dropped (closed). The plugin reconnects in 1 s and gets a fresh snapshot, which is correct self-healing and never stalls the actor. Closed followers are pruned on the next broadcast or the next `Follow` registration, using a non-blocking read that returns EOF. There are no per-follower threads (Python has one each) and no timer.
- **Slow or hostile request clients.** Each request gets its own thread with a 2 s read deadline and a 1 MiB line cap. Non-UTF-8 or unparseable input gets `unknown command`. A client that never sends `\n` costs one sleeping thread for 2 s. It no longer freezes the accept loop, which was a known Python bug.
- **Capture off the actor, Clipboard on demand.** The connection thread runs `capture_primary` and sends `Press::Selection`. The pure `decide(playing, press)` answers `NeedClipboard` only when the Selection is empty and nothing is playing; only then does the connection thread run `capture_clipboard` and send `Press::Clipboard`. A press that stops speech never waits on a clipboard read. If another press started speech between the two round trips, the follow-up decides `Keep`. Both captures are side-effect-free reads (ADR-0003).
- **Interrupt and stop latency.** `StopToken::fire()` does three things. It sets an atomic. It calls the registered ORT `RunOptions::terminate()`, which aborts an in-flight run of up to ~3 s mid-graph instead of waiting for it. It wakes the playback thread. The playback thread notices within one 10 ms poll tick. It sends SIGTERM (pw-cat must not drain), then closes stdin (pw-cat ignores TERM while stdin is open), then waits 1 s, then SIGKILLs. Nothing waits on reaping. The new Stream's player spawns immediately, while the old one is being killed.
- **Queue release before reaping.** On every exit path the playback thread drops its PCM receiver before it cuts a player. A synth thread blocked on a full queue unblocks at once, so the next Utterance's first ORT run never waits out the 1 s reap grace.
- **Single reporter.** Synth never talks to the actor. Engine errors travel down the PCM channel as `Pcm::Failed(msg)`. The playback thread alone decides the outcome: `Finished`, `Failed("player exited")`, or `Failed(engine msg)`. If its token has not fired, it runs `notify("error: …")` synchronously, so the log line exists before the state flips. Then it sends `StreamEnded`. When playback exits, the channel closes, synth's next `send` fails, and synth stops pulling batches.
- **Error** persists until `stop` or a successful `Start`. An empty press while in Error behaves as if idle and leaves the state unchanged.

**Speech pipeline and first audio** (the numbers are the prototype's measurements):

- Startup loads three things in parallel threads: the ORT fp32 session (~357 ms), the lexicon JSON parse (~145 ms), and the tagger (~5 ms). The lexicon cost hides under the session load, so there is no binary lexicon format yet. One warm-up inference runs before `bind`, because the first run is cold. The socket appears at about 0.6 to 0.8 s, and socket existence still means ready.
- `Kokoro::speak` is lazy. It normalizes and G2Ps one line at a time and feeds `batch::Batches`, which yields batch 1 before later lines are phonemized. A 50-sentence selection no longer pays for all of G2P up front.
- `Ramp` is pure and unit-tested. The first batch is capped at 32 phonemes, cut at a sentence, else a clause, else a word (~155 + 32×6 ≈ 350 ms worst case, ~200 ms typical). Each next cap is `min(510, (audio_ms(prev) − 155) / 6)`, with `audio_ms` scaled by Speed. So synthesis of batch k+1 is predicted to finish inside the playback of batch k: 32 → ~375 → 510 at speed 1.0. The prototype's balanced packing applies from the first full-size batch on.
- The speech `pw-cat` spawns when the Stream starts, in parallel with the first ORT run. The rate is always 24 kHz, so there is no reason to wait for samples. Its stream is what wakes a suspended sink. The playback thread then runs the sink probe (`pactl get-default-sink` and `pactl list sinks short`, ~10 ms, bounded at 150 ms). A SUSPENDED default sink named `bluez_output.*` or `*hdmi*` gets `wake_lead_ms` (default 650) of zeros on the player's stdin before any PCM. Every other sink, and any probe failure, gets none.
- The PCM channel is bounded at 4 batches. Synth stays at most about 4 batches ahead, so an interrupted long read wastes little CPU.

Expected first audio from pressing the key is ~15 ms (CLI, socket, `wl-paste`) + ~7 ms (G2P of line 1) + ~200 to 350 ms (first ORT run), for about 0.25 to 0.4 s. v0.5.1 measured >1.3 s on a paragraph.

**Data files are embedded** (~14.5 MB, taking the binary to ~19 MB). They are version-locked to the code that parity-tested them, a missing data file is not a possible failure, and AUR packaging is simpler. They sit in `.rodata`, so a CLI invocation never pages them in. `build.rs` compiles the misaki lexicons into sorted tables (grown per `grow_dictionary`) that `Lexicon` binary-searches in place. Parsing the JSON into `HashMap`s at load cost ~120 ms and ~70 MB of heap; the tables cost neither, for ~4 MB more binary (~2 MB more gzipped tarball), because grown keys duplicate their entries. The models (`kokoro-v1.0.onnx` fp32 310 MB, `voices-v1.0.bin`) stay downloaded under `$OMATALK_MODELS`.

**Native libraries are dlopened only by the Daemon.** ORT uses `load-dynamic` against Arch's `onnxruntime-cpu`. espeak-ng uses `libloading`. The CLI path (`version`, `config`, `speak`) touches no native library, so the plugin's `omatalk version` works even if a system package breaks. CI builds with no native libraries installed.

**Idle recycle becomes an idle rest.** Recycling existed because Python's ORT arena grew to fit the longest run and never shrank. Here every run is capped at 510 phonemes, so the arena is bounded by one max batch, but the bound is high: a soak of 30 short and 30 long Utterances, 20 stops, and 20 replacements plateaued at 1.3 GB after the first long one. A restart would release it at the cost of a ~1.2 s gap that flashes the bar red. Instead, `REST_AFTER` (60 s) after the last synthesis the synth thread calls `Engine::rest`, which runs the warm-up sentence with ORT's `memory.enable_memory_arena_shrinkage` run option; RSS drops to about 510 MB. Shrinking after every run was measured and rejected: first audio rose from at most 325 ms to 440 to 516 ms on 11 of 30 presses. The first press after a rest measured 284 to 299 ms.

**Panics.** A panic hook aborts the process for a panic on any thread except the synth thread, and systemd restarts the Daemon (`RestartSec=500ms`, gap under 1.5 s), so a half-poisoned actor never keeps running. The synth thread wraps each job in `catch_unwind`: a G2P or engine panic on odd input becomes `Pcm::Failed("internal error: ...")`, so the user gets the `error:` notify and the Daemon stays up (the engine holds no state across jobs that a panic can corrupt). The parity corpus (28,759 lines plus 2,139 held out) doubles as the no-panic fuzz set for the G2P port.

**Tests.** Two tiers, and each Python test has a stated fate:

- `cargo test`: pure units (`protocol`, `cli::parse`, `decide`, `Speed::parse`, config set preserving other bytes and comments, `Ramp`, `normalize` 22 cases, `trim`). Actor tests drive a real `Actor` with `RecordingEngine` and the existing shell fakes in `tests/fakes/`. These are ports of `test_utterance_binding.py` and `test_player.py`, which import Python internals and are deleted.
- pytest, black-box, kept as the contract lever: `test_socket.py`, `test_cli.py`, `test_installer.py`, `test_site.py`, and `test_pack_src.py` run against `target/debug/omatalk` (session fixture runs `cargo build`). `bin/omatalk` becomes `exec "$REPO/target/debug/omatalk" "$@"`, and `bin/omatalkd` becomes `... daemon`. `OMATALK_TEST_FAKE_ENGINE=1` keeps its meaning, which is to skip model, lexicon, and dlopen. `test_site.py` reads defaults from `omatalk config get --json`. `test_requirements.py` is deleted.
- Parity: `examples/parity.rs` (`cargo run --release --example parity -- tok|tag|e2e corpus.jsonl`). A 500-line sample corpus is checked in and runs as an `#[ignore]` test (`cargo test --release -- --ignored`). The full 37 MB corpus plus `tools/parity/dump_misaki.py` regenerate from Python misaki on demand. Golden audio against kokoro-onnx is dropped, because batching deliberately differs.

**Distribution and migration** (per outcome-oriented-execution and migrate-callers-then-delete-legacy-apis, in one wave, with no Python fallback):

- The release tarball `omatalk-x86_64.tar.gz` contains `omatalk` and `omatalk.service`. It keeps the assets `install.sh` and `uninstall.sh`. `build.sh` runs `cargo build --release --locked`, packs deterministically, and pins tag and sha. The version source becomes `Cargo.toml`, and `make bump` edits `Cargo.toml` and `Cargo.lock`.
- Unit: `ExecStart=%h/.local/bin/omatalk daemon`, `Restart=always`, `RestartSec=500ms`, same name and target.
- `install.sh` has the same skeleton and pins. Package dependencies become `curl pipewire wl-clipboard onnxruntime-cpu espeak-ng libnotify`, and `python` and `uv` are dropped. It downloads the fp32 model (new sha, from the `model-files-v1.0` release), stops the unit, and installs the binary to `~/.local/bin/omatalk` by writing a temp file and renaming it. That is a regular file, so the plugin's `test -f` passes, and a rename cannot tear the running inode. It then removes `$OMATALK_HOME/{venv,src}`, `~/.local/bin/omatalkd`, and the old fp16 model, and never touches `config.toml`. A v0.5.1 install upgrades by running this script (`omatalk upgrade` from the Python CLI fetches it).
- `omatalk uninstall` writes an embedded copy of `uninstall.sh` (`include_str!`) to a temp file and execs bash on it. The "shipped uninstaller missing" branch disappears. `uninstall.sh` keeps the old `pkill` patterns and adds `[o]matalk daemon`.
- The rewrite ships as a plain `v0.9.0` release, cut from `master` after the `rust-rewrite` integration branch merges. Until then `releases/latest` keeps serving 0.5.1 to the site. Local dogfooding uses `scripts/dev-install.sh`: `cargo build --release`, an atomic copy to `~/.local/bin/omatalk`, and a unit restart.
- Deleted in the first implementation wave: `daemon/`, `requirements.txt`, the `[project]` packaging in `pyproject.toml` (it keeps only the dev group for pytest, ruff, and numpy), and `tests/test_requirements.py`. ADR-0002 gets a superseding ADR-0005 ("Rust Daemon, misaki G2P, fp32"). ADR-0001 gets a revision for fp32.

## Synthesis decision

Arena with two runners on the architect rubric (`contract fidelity, interface depth, concurrency, latency, testability, migration`). Candidate A (opus: one crate, one binary, std threads, actor) is the base; candidate B (sonnet: workspace with a `speech` crate, separate `omatalk`/`omatalkd`, tokio) scored lower. The sonnet cross-judge scored A 29/30 and B 16/30 and recommended A; the orchestrator's own read agreed. Deciding facts: A aborts an in-flight ORT run on Interrupt (B only skips the next batch, up to ~3 s late); A broadcasts every transition (B's `watch` coalesces for slow followers); A's JSON `speak` payload round-trips any text (B's `\x1f` + `\n`-escape corrupts a literal backslash-n); A specifies migration and distribution end to end (B does not).

Grafted from B:
- Clipboard on demand (B's `NeedClipboard` two-phase decision). A read the Clipboard whenever the Selection was empty, so a stop press could wait up to 2 s on `wl-paste`. Now `decide` returns `NeedClipboard` and the connection thread reads the Clipboard only then.

Changed by the orchestrator: crash-only `panic = "abort"` became a panic hook that aborts outside the synth thread plus `catch_unwind` per synth job, so odd input produces an `error:` notify instead of a silent restart.

Rejected from B: tokio (ORT and espeak are blocking and thread-affine; the load-bearing work would be `spawn_blocking` anyway), two binaries (doubles install and AUR artifacts for no separation `omatalk daemon` lacks), the `\x1f` wire format, `watch` for follow, and eager whole-text G2P before the first batch. Deferred: B's physical crate split (`speech` cannot see sockets). Worth revisiting once the single crate settles; not worth slowing the first wave.

## Tradeoffs accepted

- We accept a 10 ms poll tick in the playback writer (inaudible, and only while speaking) in exchange for not needing a wake-pipe or eventfd per Stream. Idle has zero wakeups.
- We accept thread-per-request (unbounded under a local flood) in exchange for no pool and a trivially correct deadline per client. Followers do not hold threads.
- We accept dropping a follower that stops reading, instead of buffering for it. The plugin's 1 s reconnect makes this invisible.
- We accept a mid-sentence cut in the first batch of a long first sentence (a small prosody seam) in exchange for ~0.25 to 0.4 s first audio instead of >1 s.
- We accept a ~16 MB binary with embedded data, where a CLI call pages in only what it touches, in exchange for no data-file install path or version skew.
- We accept a second actor round trip on a press with an empty Selection while idle, in exchange for an actor that never runs a subprocess and a stop press that never waits on the Clipboard.
- We accept that the synchronous error notify runs on the playback thread, delaying the `error` state by the notify-send runtime (bounded at 2 s), in exchange for "notify logged before state flips", which tests rely on.
- We accept a restart for panics outside synthesis in exchange for no poisoned-state recovery code; synthesis panics are caught per job because odd input is their likely cause.
- We accept that `OMATALK_TEST_FAKE_ENGINE` stays a production env seam, because the black-box suite needs a Daemon without models.

## Alternatives considered

- **Shared state behind `Mutex` + `Condvar`** (Python's shape in Rust). The surface is similar, but every thread must learn the lock order, and "no lost transition" relies on holding one lock across a snapshot write. That is the exact prose invariant the actor makes structural. It lost on hidden complexity exposed to every call site.
- **An async runtime (tokio) with one task per connection and follower.** Cancellation is simpler to express, but ORT and espeak are blocking and thread-affine, so the load-bearing work runs in `spawn_blocking` anyway. It adds a runtime, a second concurrency model, and ~2 MB for a Daemon with at most a handful of connections.
- **Two binaries (`omatalk` + `omatalkd`).** This matches today's layout, but it doubles install, launcher, and AUR artifacts and forces a shared library crate for types both need. `omatalk daemon` gives the same separation with one file.
- **Synth writes PCM straight into pw-cat** (no playback thread). This has fewer threads, but a full pipe (1.3 s of audio) blocks synthesis, which defeats overlap, and interrupt then waits on a blocked write.
- **Keeping the idle recycle.** It costs a restart gap that flashes the bar red; the idle rest releases the same memory in place.
- **Disabling the CPU arena.** It returns 450 to 700 MB between Utterances but makes long text about 15% slower and raises the peak during speech.

## Implementation reconciliation

Unit 1 (core: CLI, protocol, config, exec, Daemon actor, Stream, fakes):

- `stop` replies `ok` before the player is cut. The actor drops the Stream and answers; the playback thread sends SIGTERM within one TICK. Python reaped before replying. `tests/test_socket.py::test_stop_cuts_playback` now waits for the `killed` line instead of reading the log at once. This follows "nothing waits on reaping".
- `Paths.home` is gone. The embedded `uninstall.sh` reads `OMATALK_HOME` itself, and nothing else used the field.
- `exec::capture` returns `None` on non-UTF-8 output instead of decoding lossily. An unreadable Source is an empty Source, and replacement characters would be spoken.
- Closed followers are detected with a non-blocking read into a small buffer, not a zero-byte read. A zero-length read returns 0 on a live socket too. Followers send nothing after `follow`, so the bytes it consumes are never needed.
- The abort-outside-synth panic hook is installed by `omatalk daemon` only. A CLI panic keeps Rust's default exit.
- `RecordingEngine` takes a `before(piece_index)` hook and a `fail_at` index instead of a built-in hold/release gate. Tests build holds, waits on player logs, and failures from those two fields.
- A hand-edited `lang` other than `en-us` loads as en-us (orchestrator, at integration): failing the load would silence every press for a v0.5 user who set `lang = "en-gb"`. The ported binding test changes voice and speed only. Porting misaki's gb lexicons stays open.

Unit 2 (speech: G2P port, Kokoro engine, batching, Ramp):

- The Ramp constants are remeasured through this engine (`examples/speak.rs --rate`, README paragraphs and prose, Ryzen 7840HS, onnxruntime 1.29). They are now 190 ms floor, 11 ms per phoneme of synthesis, and 58 ms of audio per phoneme. The design's 155/6/75 overestimated audio by 30% and underestimated long-batch synthesis by 80%. With them, the first README paragraph split into batches of 26 and 215 phonemes, and batch 2 arrived 0.8 to 0.9 s after batch 1's audio ran out. The ramp is now 32 → ~151 → 510, and the same paragraph and the 11-line "How it works" section play with no gap.
- Full-size batches use the prototype's balanced packing over a window, not the whole Utterance. `Batches` pulls lines until more than 510 phonemes are pending, packs them, and carries the last packed batch forward while lines remain. Balancing over the whole text would need G2P of all of it before batch 1.
- `Espeak` is a process-wide `OnceLock<Mutex<Espeak>>`, not a `Send + !Sync` handle created once. `G2p` holds `&'static Mutex<Espeak>`. The mutex gives the same guarantee (no two threads in espeak-ng's globals) and lets more than one `G2p` exist in a process, which the parity example and the parallel `cargo test` runner need. Locking once per line costs nothing measurable.
- Startup runs two threads, not three. The ORT session loads on one. `G2p::load` (tokenizer, tagger, both lexicons, espeak dlopen) runs on the other in 150 to 210 ms, well under the session's ~400 ms.
- libonnxruntime is found in this order: `ORT_DYLIB_PATH` if set, else `libonnxruntime.so.1`, else `libonnxruntime.so`, both through the dynamic loader's search path (Arch's `onnxruntime-cpu` puts them in /usr/lib). Each candidate is probed with a plain dlopen before `ort::init_from` runs once. ort 2.0.0-rc.13's `OnceLock` marks itself initialized after a failed init, so a second `init_from` would read uninitialized memory.
- `warm()` runs "Warm up, Omatalk." through G2P (normalize, tagger, lexicon, and espeak for the unknown word) and ORT with a zero style vector. It needs no voice and warms the regex tables, not only the ORT session.
- The espeak port keeps only tie mode, the one misaki uses. The prototype's non-tie phonemizer path and its `OMATALK_FIX_SPLIT` experiment are gone.
- `G2p::phonemize(text, unk)` is public and skips normalization. It is the parity target (Python misaki's `G2P(text)`). `G2p::line` is normalize plus `phonemize` with an empty `unk`.
- The sample corpus is `tools/parity/sample.jsonl` (450 rows of `misaki.jsonl`, 50 of the held-out set), not `tests/parity/`, because `tests/` belongs to Unit 1. Its floor is 499/500. The miss is misaki's markdown link syntax, which the port skips on purpose.
- `examples/speak.rs` is a second lever. It loads, warms, speaks, and reports first audio, per-batch gaps, RTF, stop latency, and peak RSS, and `--rate` refits the Ramp constants.
- `.gitignore` ignores `*.bin` for models, so it gains `!/data/tagger.bin`.
- Measured before the lexicon tables (which cut about 80 MB throughout), peak RSS was about 600 MB after a short sentence, 750 MB after one paragraph, and 1.0 GB after 42 s of speech in 510-phoneme batches. The 510 cap bounds the arena, but the bound is high, so the idle rest shrinks it back to about 510 MB.

Wake lead (replaces the wake shim, which resolves the open question "does spawning the speech `pw-cat` early make the wake shim redundant?"):

- The shim did not stop the clip. On a cold press to a suspended Bluetooth speaker, the shim brought the sink to RUNNING and the BlueZ transport to `active` at ~+50 ms, and first speech was written at ~+215 ms. The speaker then unmutes 375 to 550 ms after the link is up, and no software signal marks that. The first word was lost. The Python Daemon did not clip only because its first speech came at ~+665 ms.
- An ear test prepended N ms of zeros to the speech stream on a cold press: 0, 200, and 400 ms clipped, and 600 ms was clean. `wake_lead_ms` defaults to 650 for margin, accepts 0 to 2000, and 0 disables it.
- The lead is played only on a suspended Bluetooth or HDMI default sink. Analog sinks suspend too but do not clip, and a warm sink needs nothing, so both keep ~210 ms first audio. DisplayPort sinks are named `hdmi` in PipeWire and are included on purpose.
- With one player per Utterance, a cold press measured sink RUNNING at +51 ms, transport `active` at +52 ms, the lead's first bytes at +23 ms, and first speech written at +211 ms, queued behind the lead. A warm press wrote speech as its first byte at +210 ms. The speech player's own stream wakes the sink as early as the shim did, so the shim is deleted.
- The probe is `sink_probe` in config (default `["pactl"]`, the Daemon appends the subcommand), the same test seam as `player`. `pactl` comes from `libpulse`, which `espeak-ng` already pulls in through `pcaudiolib`, so `install.sh` needs no new package.

Unit 3 (distribution and migration: installer, uninstaller, unit, release scripts, CI, docs, AUR draft):

- Model pins are verified against upstream. `kokoro-v1.0.onnx` from `model-files-v1.0` is 325,532,387 bytes with sha256 `7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5`. `voices-v1.0.bin` is `bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d`, the same file v0.5 fetched from `model-files-v1.1`, so an upgrade downloads only the model. Both hashes come from streaming the release URLs through `sha256sum` and match the prototype's local copies.
- The tarball also holds `LICENSE`. The AUR package must install the MIT license text, and the tarball is its only source.
- `scripts/build.sh --pack-only` packs the binary already in `target/release` and does not build. `tests/pack.rs` uses it. The pack command lives only in `build.sh`.
- `tests/pack.rs` packs a fake binary in a temporary copy of the tree through `CARGO_TARGET_DIR`. A test that packed in the real tree would overwrite a tarball left by `build.sh`.
- `scripts/verify.sh` is gone. Once the tarball held a compiled binary, it could only repack the binary `build.sh` had just packed and compare it to the pin `build.sh` had just written. `release.sh` now runs `cargo test --locked` and `make lint` before `build.sh`, so a red suite stops the release before `install.sh` changes.
- `install.sh` downloads and unpacks the tarball into `$OMATALK_HOME/.release.XXXXXX` before it downloads models or stops the Daemon. A checksum mismatch leaves the running install untouched and the Daemon running. The staging directory is removed on exit. The migration step also deletes a leftover `$OMATALK_HOME/omatalk-src.tar.gz`.
- `uninstall.sh` does not add a `[o]matalk daemon` pattern. The existing `[o]matalk.daemon` is a regex, and its dot matches the space in `omatalk daemon`. `uninstall_removes_a_rust_install_and_its_models` in `tests/installer.rs` checks that a pattern passed to `pkill` matches `~/.local/bin/omatalk daemon`.
- Local dogfooding is `scripts/dev-install.sh`, not `make dev-install`. AGENTS.md keeps build and install scripts out of Make. The old `dev-install`, `dev-restart`, and `dev-uninstall` targets are gone. The script enables the unit and warns when the fp32 model is missing. It downloads nothing.
- The pytest suite was later ported to Rust integration tests in `tests/*.rs`, so `cargo test` is the whole suite and Python is out of the dev loop. `pyproject.toml`, `uv.lock`, `bin/omatalk`, and `bin/omatalkd` are gone. Lint is `cargo fmt --check` plus clippy. The scripts in `tools/parity/` are standalone `uv run --script` files with PEP 723 pins, and nothing lints them.
- Units 1 and 2 were not rustfmt-formatted (215 hunks under default rustfmt). `cargo fmt` ran once over the crate with default settings, and no `rustfmt.toml` was added. The change is formatting only. `cargo test` and clippy pass after it.

## Open questions and risks

- Should `lang = "en-gb"` (hand-edited) fail with `error: lang en-gb is not supported`, or should the port add misaki's `gb_gold`/`gb_silver` before 0.9.0? British voices (`bf_*`, `bm_*`) currently get US phonemes unless the user set `lang`.
  - The lexicon tables already fit a second dialect. Add `gb_gold` and `gb_silver` to the `build.rs` loop and have `Lexicon` pick a table pair by `lang`. Table pages load only when read, so the unused dialect costs no RSS. It adds about 10 MB to the binary (about 1.8 MB gzipped), because the tables store the copies from `grow_dictionary` in full. That is the point to revisit resolving those copies at lookup time. The larger cost is porting misaki's `british=True` logic in `en.py`, with its own parity corpus.
- AUR (`omatalk-bin`): the frozen plugin probes `~/.local/bin/omatalk`, which a package cannot create. Do we accept a per-user step (`install.sh` detects `/usr/bin/omatalk`, symlinks the launcher, fetches models, binds F8) or plan a plugin release that also probes `command -v omatalk`? Where do the 310 MB of models live for AUR: a separate `omatalk-models` package in `/usr/share/omatalk/models` as a second search path, or a per-user download?
  - The draft in `packaging/aur/omatalk-bin/` installs `/usr/bin/omatalk`, a user unit in `/usr/lib/systemd/user/` with `ExecStart=/usr/bin/omatalk daemon`, and the license. `makepkg` builds it from a local tarball. It does not solve the launcher probe or the models, and `sha256sums` is `SKIP` until a release exists.
  - Proposal for the probe: a plugin release that probes `~/.local/bin/omatalk`, then `/usr/bin/omatalk`, and runs whichever exists. Until then, a package user runs `ln -s /usr/bin/omatalk ~/.local/bin/omatalk`. A symlink passes `test -f`. `install.sh` must not do this on its own, because it replaces the launcher with a regular file and would shadow the package's binary.
  - Proposal for models: a separate `omatalk-models` package (arch `any`, about 355 MB) that installs to `/usr/share/omatalk/models`. The Daemon and `config voices` search `$OMATALK_MODELS`, then `~/.local/share/omatalk/models`, then `/usr/share/omatalk/models`. The packaged unit then drops its `OMATALK_MODELS` line. This needs a change in `Paths::from_env`, which this unit did not make.
- An optional smaller fp16 download (`kokoro-v1.0.fp16.onnx`, 156 MB against 310 MB) as an installer choice is a possible follow-up for slow connections. Its measured cost on the 7840HS is about 100 ms slower first audio (278 ms against 176 ms for "Hello.") and a model load about 2x slower (693 ms against 357 ms), because the CPU has no native fp16 and onnxruntime upcasts on every run. fp32 stays the default.
- `omatalk upgrade` fetches the site installer, which runs the installer from `releases/latest`. On a dev install of the branch, that is the 0.5.1 Python installer, so `upgrade` downgrades a dogfooder to Python. The 0.5.1 installer overwrites the launcher and the unit, so the downgrade works, but it is a surprise. Accepted until 0.9.0 ships; dogfooders update with `scripts/dev-install.sh`.
- `scripts/release.sh` publishes only from `master`, and GitHub Pages deploys `public/` from `master`. `rust-rewrite` is the integration branch: follow-up PRs target it, and it merges to `master` once, when 0.9.0 is ready. Merging publishes the 0.9 site copy (Rust daemon, about 355 MB of models) while `releases/latest` still installs 0.5.1, so cut the 0.9.0 release right after the merge.
- Per-line G2P of a single huge paragraph costs ~7 ms per sentence before batch 1. Does G2P per sentence keep parity? The tagger's receptive field is ~4 tokens. Run `examples/parity e2e` in sentence mode to find out. If it matches, batch 1 always waits for one sentence only.
- The Ramp constants (155 ms, 6 ms per phoneme, ~75 ms of audio per phoneme) come from one Ryzen 7840HS. Is a slower CPU going to gap between batches 1 and 2? A cheap fix is measuring the real synth rate per run and feeding it to the Ramp. Deferred until a gap is heard.

## Next implementation step

Land the scaffold commit. That is `Cargo.toml` at 0.9.0, `protocol.rs`, `cli.rs`, `config.rs`, `voices.rs`, `exec.rs`, and `daemon.rs` with `FakeEngine`, and a `stream.rs` that plays fake PCM through the shell player. Repoint `bin/omatalk` so that `test_socket.py` and `test_cli.py` pass against the Rust binary with only their invocation swapped (`sys.executable -m daemon.cli` becomes `bin/omatalk`), before any speech code moves in.

Sketch status: `sketch/` passes `cargo check --offline --all-targets` with dead-code warnings only (the bodies are `todo!()`). The four `decide()` unit tests in `daemon.rs` run and pass. `ort`, `libloading`, `fancy-regex`, `unicode-normalization`, and `npyz` are listed but commented out in `Cargo.toml`, so the check can run offline.
