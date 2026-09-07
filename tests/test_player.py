import os
import time

import numpy as np

from daemon.player import RATE, WAKE_MS, Player


def make_echo_player(tmp_path):
    # Per-pid files — wake and speech are separate processes (separate
    # PipeWire streams in production).
    script = tmp_path / "echo-player"
    args_log = tmp_path / "args.log"
    script.write_text(
        "#!/bin/sh\n"
        f'printf "%s\\n" "$*" >> "{args_log}"\n'
        f'cat >> "{tmp_path}/captured.$$.bin"\n'
    )
    script.chmod(0o755)

    def captured_for(pid):
        return tmp_path / f"captured.{pid}.bin"

    return {"player": [str(script)]}, captured_for, args_log


def pcm_bytes(samples):
    return (
        (np.clip(np.asarray(samples, dtype=np.float64), -1.0, 1.0) * 32767)
        .astype(np.int16)
        .tobytes()
    )


def wake_bytes():
    return np.zeros(int(RATE * WAKE_MS / 1000), dtype=np.int16).tobytes()


def wait_path(path, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists() and path.stat().st_size:
            return
        time.sleep(0.01)
    raise AssertionError(f"{path} stayed empty")


def test_play_feeds_raw_pcm_via_stdin_with_rate_and_channel_args(tmp_path):
    cfg, captured_for, args_log = make_echo_player(tmp_path)
    samples = [0.5, -0.5, 0.25, -0.25]
    player = Player(cfg)
    player.begin()
    assert player.play(samples, 24000)
    assert player.finish()
    captured = captured_for(player._proc.pid)
    player.stop()

    assert captured.read_bytes() == pcm_bytes(samples)
    assert "--rate 24000 --channels 1 -" in args_log.read_text().splitlines()


def test_successive_plays_concatenate_on_one_player(tmp_path):
    cfg, captured_for, args_log = make_echo_player(tmp_path)
    first = [0.5, -0.5]
    second = [0.25, -0.25]
    player = Player(cfg)
    player.begin()
    assert player.play(first, 24000)
    assert player.play(second, 24000)
    assert player.finish()
    captured = captured_for(player._proc.pid)
    player.stop()

    assert captured.read_bytes() == pcm_bytes(first) + pcm_bytes(second)
    assert args_log.read_text().splitlines()[-1] == "--rate 24000 --channels 1 -"


def test_play_returns_without_waiting_for_a_slow_reader(tmp_path):
    script = tmp_path / "slow-player"
    captured = tmp_path / "captured.bin"
    script.write_text(f'#!/bin/sh\nsleep 1\ncat >> "{captured}"\n')
    script.chmod(0o755)
    captured.write_bytes(b"")
    cfg = {"player": [str(script)]}
    # Large enough to exceed a typical OS pipe buffer (64KB) if play() wrote
    # to stdin synchronously instead of from a background thread — that
    # regression would make this call itself block for ~1s.
    samples = [0.1] * 200_000

    player = Player(cfg)
    player.begin()
    start_at = time.monotonic()
    assert player.play(samples, 24000)
    elapsed = time.monotonic() - start_at

    assert elapsed < 0.5, "play() must not block on a slow/idle reader"
    assert player.finish()
    player.stop()
    assert captured.read_bytes().endswith(pcm_bytes(samples))


def test_first_play_does_not_wait_for_wake_to_exit(tmp_path):
    stamp = tmp_path / "speech"
    pidfile = tmp_path / "wake.pid"
    captured = tmp_path / "captured.bin"
    script = tmp_path / "player"
    script.write_text(
        "#!/bin/sh\n"
        f'stamp="{stamp}"\n'
        f'pidfile="{pidfile}"\n'
        f'captured="{captured}"\n'
        'if [ -f "$stamp" ]; then\n'
        '  cat >> "$captured"\n'
        "else\n"
        '  echo $$ > "$pidfile"\n'
        '  touch "$stamp"\n'
        "  trap 'sleep 2; exit 0' TERM\n"
        "  cat >/dev/null\n"
        "fi\n"
    )
    script.chmod(0o755)
    captured.write_bytes(b"")
    player = Player({"player": [str(script)]})
    player.begin()
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        if stamp.exists():
            break
        time.sleep(0.01)
    else:
        raise AssertionError("wake player never started")
    samples = [0.5, -0.5]
    start_at = time.monotonic()
    assert player.play(samples, 24000)
    elapsed = time.monotonic() - start_at
    assert elapsed < 0.5, "first play() must not wait for the wake shim to die"
    assert player.finish()
    player.stop()
    assert captured.read_bytes().endswith(pcm_bytes(samples))
    pid = int(pidfile.read_text())
    try:
        os.kill(pid, 0)
        still = True
    except OSError:
        still = False
    assert not still, "stop() must reap a wake shim that ignored SIGTERM"


def test_begin_writes_one_quantum_of_silence(tmp_path):
    # No play() here, so the wake kick is the only process that ever runs —
    # its capture file is the only one that can appear.
    cfg, _captured_for, args_log = make_echo_player(tmp_path)
    player = Player(cfg)
    player.begin()
    wait_path(args_log)
    deadline = time.monotonic() + 5
    captured = None
    while time.monotonic() < deadline:
        matches = list(tmp_path.glob("captured.*.bin"))
        if matches and matches[0].stat().st_size >= len(wake_bytes()):
            captured = matches[0]
            break
        time.sleep(0.01)
    player.stop()

    assert captured is not None, "wake proc never wrote its capture file"
    assert captured.read_bytes() == wake_bytes()
    assert args_log.read_text().strip() == f"--rate {RATE} --channels 1 -"


def test_stop_cuts_without_draining_stdin(tmp_path):
    log = tmp_path / "log.txt"
    log.write_text("")
    script = tmp_path / "drain-player"
    script.write_text(
        "#!/usr/bin/env python3\n"
        "import os, signal, sys, time\n"
        f"log = {str(log)!r}\n"
        "def stamp(msg):\n"
        "    with open(log, 'a') as f:\n"
        "        f.write(msg + '\\n')\n"
        "        f.flush()\n"
        "def on_term(*_):\n"
        "    stamp('killed')\n"
        "    os._exit(0)\n"
        "signal.signal(signal.SIGTERM, on_term)\n"
        "stamp('start')\n"
        "while True:\n"
        "    b = sys.stdin.buffer.read(1)\n"
        "    if not b:\n"
        "        stamp('drained')\n"
        "        break\n"
        "    time.sleep(0.001)\n"
    )
    script.chmod(0o755)
    player = Player({"player": [str(script)]})
    player.begin()
    assert player.play([0.1] * 50_000, 24000)
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        if "start" in log.read_text():
            break
        time.sleep(0.01)
    else:
        raise AssertionError(f"player never started: {log.read_text()!r}")
    start_at = time.monotonic()
    player.stop()
    elapsed = time.monotonic() - start_at

    assert elapsed < 0.5, "stop() must SIGTERM, not wait for pw-cat to drain stdin"
    text = log.read_text()
    assert "killed" in text
    assert "drained" not in text


def test_stop_eof_after_term_lets_pw_cat_exit(tmp_path):
    log = tmp_path / "log.txt"
    log.write_text("")
    script = tmp_path / "ignore-term"
    script.write_text(
        "#!/usr/bin/env python3\n"
        "import signal, sys\n"
        f"log = {str(log)!r}\n"
        "def stamp(msg):\n"
        "    with open(log, 'a') as f:\n"
        "        f.write(msg + '\\n')\n"
        "        f.flush()\n"
        "signal.signal(signal.SIGTERM, signal.SIG_IGN)\n"
        "stamp('start')\n"
        "sys.stdin.buffer.read()\n"
        "stamp('eof')\n"
    )
    script.chmod(0o755)
    player = Player({"player": [str(script)]})
    player.begin()
    assert player.play([0.1] * 24, 24000)
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        if "start" in log.read_text():
            break
        time.sleep(0.01)
    else:
        raise AssertionError(f"player never started: {log.read_text()!r}")
    start_at = time.monotonic()
    player.stop()
    elapsed = time.monotonic() - start_at

    assert elapsed < 0.5, "stop() must close stdin after SIGTERM so wait() can finish"
    assert "eof" in log.read_text()


def test_finish_false_when_speech_process_already_dead(tmp_path):
    script = tmp_path / "exit-now"
    script.write_text("#!/bin/sh\nexit 1\n")
    script.chmod(0o755)
    player = Player({"player": [str(script)]})
    player.begin()
    alive = player.play([0.1] * 24, 24000)
    time.sleep(0.1)
    finished = player.finish() if alive else False
    player.stop()
    assert not alive or not finished


def test_stop_then_play_and_finish_are_not_error(tmp_path):
    cfg, _captured_for, args_log = make_echo_player(tmp_path)
    player = Player(cfg)
    player.begin()
    wait_path(args_log)
    player.stop()
    assert player.play([0.1], 24000)
    assert player.finish()


def test_play_false_when_speech_player_cannot_start(tmp_path):
    missing = tmp_path / "no-such-player"
    player = Player({"player": [str(missing)]})
    player.begin()
    assert player.play([0.1], 24000) is False
    player.stop()
