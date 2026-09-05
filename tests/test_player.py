import time

import numpy as np

from daemon.player import close_stdin, feed, start


def make_echo_player(tmp_path):
    script = tmp_path / "echo-player"
    captured = tmp_path / "captured.bin"
    args_log = tmp_path / "args.log"
    script.write_text(
        "#!/bin/sh\n"
        f'printf "%s\\n" "$*" >> "{args_log}"\n'
        f'cat > "{captured}"\n'
    )
    script.chmod(0o755)
    return {"player": [str(script)]}, captured, args_log


def pcm_bytes(samples):
    return (np.clip(np.asarray(samples, dtype=np.float64), -1.0, 1.0) * 32767).astype(np.int16).tobytes()


def test_start_feeds_raw_pcm_via_stdin_with_rate_and_channel_args(tmp_path):
    cfg, captured, args_log = make_echo_player(tmp_path)
    samples = [0.5, -0.5, 0.25, -0.25]

    proc = start(cfg, 24000)
    feed(proc, samples).join(timeout=5)
    close_stdin(proc)
    proc.wait(timeout=5)

    assert captured.read_bytes() == pcm_bytes(samples)
    assert args_log.read_text().strip() == "--rate 24000 --channels 1 -"


def test_successive_feeds_concatenate_on_one_player(tmp_path):
    cfg, captured, args_log = make_echo_player(tmp_path)
    first = [0.5, -0.5]
    second = [0.25, -0.25]

    proc = start(cfg, 24000)
    feed(proc, first).join(timeout=5)
    feed(proc, second).join(timeout=5)
    close_stdin(proc)
    proc.wait(timeout=5)

    assert captured.read_bytes() == pcm_bytes(first) + pcm_bytes(second)
    assert args_log.read_text().splitlines() == ["--rate 24000 --channels 1 -"]


def test_feed_returns_without_waiting_for_a_slow_reader(tmp_path):
    script = tmp_path / "slow-player"
    captured = tmp_path / "captured.bin"
    script.write_text(f'#!/bin/sh\nsleep 1\ncat > "{captured}"\n')
    script.chmod(0o755)
    cfg = {"player": [str(script)]}
    # Large enough to exceed a typical OS pipe buffer (64KB) if feed() wrote
    # to stdin synchronously instead of from a background thread — that
    # regression would make this call itself block for ~1s.
    samples = [0.1] * 200_000

    proc = start(cfg, 24000)
    start_at = time.monotonic()
    feeder = feed(proc, samples)
    elapsed = time.monotonic() - start_at

    assert elapsed < 0.5, "feed() must not block on a slow/idle reader"
    feeder.join(timeout=5)
    close_stdin(proc)
    proc.wait(timeout=5)
    assert captured.read_bytes() == pcm_bytes(samples)
