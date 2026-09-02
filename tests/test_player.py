import time

import numpy as np

from omatalk.player import play


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


def test_play_streams_raw_pcm_via_stdin_with_rate_and_channel_args(tmp_path):
    cfg, captured, args_log = make_echo_player(tmp_path)
    samples = [0.5, -0.5, 0.25, -0.25]

    proc = play(cfg, samples, 24000)
    proc.wait(timeout=5)

    assert captured.read_bytes() == pcm_bytes(samples)
    assert args_log.read_text().strip() == "--rate 24000 --channels 1 -"


def test_play_returns_without_waiting_for_a_slow_reader(tmp_path):
    script = tmp_path / "slow-player"
    captured = tmp_path / "captured.bin"
    script.write_text(f'#!/bin/sh\nsleep 1\ncat > "{captured}"\n')
    script.chmod(0o755)
    cfg = {"player": [str(script)]}
    # Large enough to exceed a typical OS pipe buffer (64KB) if play() wrote
    # to stdin synchronously instead of from a background thread — that
    # regression would make this call itself block for ~1s.
    samples = [0.1] * 200_000

    start = time.monotonic()
    proc = play(cfg, samples, 24000)
    elapsed = time.monotonic() - start

    assert elapsed < 0.5, "play() must not block on a slow/idle reader"
    proc.wait(timeout=5)
    assert captured.read_bytes() == pcm_bytes(samples)
