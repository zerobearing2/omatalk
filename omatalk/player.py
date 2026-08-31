import subprocess
import tempfile
import wave

import numpy as np


def play(cfg: dict, samples, rate: int):
    pcm = (np.clip(np.asarray(samples), -1.0, 1.0) * 32767).astype(np.int16)
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        path = f.name
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(pcm.tobytes())
    return subprocess.Popen(
        [*cfg["player"], path],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
