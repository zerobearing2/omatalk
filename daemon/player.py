import subprocess
import threading

import numpy as np

# Kokoro is always 24 kHz. A 1ms chirp is 24 samples — below a PipeWire
# quantum — so the wake burst is one quantum of silence. Zeros unsuspend
# the sink; a tone at this rate would be ≤12 kHz and audible. The wake
# stream is a throwaway pw-cat, not the Utterance player: a new stream
# still drops its own start if the sink is SUSPENDED, but not if it is
# already RUNNING (the SoundCore + Brave case).
RATE = 24000
WAKE_MS = 48


def start(cfg: dict, rate: int):
    return subprocess.Popen(
        [*cfg["player"], "--rate", str(rate), "--channels", "1", "-"],
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def wake_pcm(rate=RATE):
    return np.zeros(int(rate * WAKE_MS / 1000), dtype=np.float64)


def wake(cfg):
    proc = start(cfg, RATE)
    feed(proc, wake_pcm(), rate=RATE)
    return proc


def reap(proc):
    if proc is None or proc.poll() is not None:
        return
    close_stdin(proc)
    proc.terminate()
    try:
        proc.wait(timeout=1)
    except subprocess.TimeoutExpired:
        proc.kill()


def feed(proc, samples, rate=RATE):
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
