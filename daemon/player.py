import subprocess
import threading

import numpy as np


def start(cfg: dict, rate: int):
    return subprocess.Popen(
        [*cfg["player"], "--rate", str(rate), "--channels", "1", "-"],
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def feed(proc, samples):
    pcm = (np.clip(np.asarray(samples), -1.0, 1.0) * 32767).astype(np.int16)

    # Fed from a thread, not written inline here: a multi-second utterance
    # exceeds the OS pipe buffer, so a synchronous write would block the
    # caller until the player drains it — defeating _run()'s overlap of this
    # sentence's playback with the next sentence's synthesis. stdin stays
    # open so later sentences can append to the same player process.
    def write():
        try:
            proc.stdin.write(pcm.tobytes())
            proc.stdin.flush()
        except (BrokenPipeError, OSError):
            pass

    thread = threading.Thread(target=write, daemon=True)
    thread.start()
    return thread


def close_stdin(proc):
    try:
        proc.stdin.close()
    except OSError:
        pass
