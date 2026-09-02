import subprocess
import threading

import numpy as np


def play(cfg: dict, samples, rate: int):
    pcm = (np.clip(np.asarray(samples), -1.0, 1.0) * 32767).astype(np.int16)
    proc = subprocess.Popen(
        [*cfg["player"], "--rate", str(rate), "--channels", "1", "-"],
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    # Fed from a thread, not written inline here: a multi-second utterance
    # exceeds the OS pipe buffer, so a synchronous write would block this
    # call until the player drains it — defeating _run()'s overlap of this
    # sentence's playback with the next sentence's synthesis.
    def feed():
        try:
            proc.stdin.write(pcm.tobytes())
        except (BrokenPipeError, OSError):
            pass
        finally:
            try:
                proc.stdin.close()
            except OSError:
                pass

    threading.Thread(target=feed, daemon=True).start()
    return proc
