import subprocess
import threading

import numpy as np

# One PipeWire quantum of zeros: shorter does not link, and a tone at
# 24 kHz would be audible.
RATE = 24000
WAKE_MS = 48


def _start(cfg: dict, rate: int):
    return subprocess.Popen(
        [*cfg["player"], "--rate", str(rate), "--channels", "1", "-"],
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def _wake_pcm(rate=RATE):
    return np.zeros(int(rate * WAKE_MS / 1000), dtype=np.float64)


def _reap(proc):
    # Terminate first: close_stdin would let pw-cat drain the pipe (up to
    # a second of already-fed PCM) before dying, which is finish()'s job.
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=1)
    except subprocess.TimeoutExpired:
        proc.kill()
    _close_stdin(proc)


def _drop(proc):
    # Wake teardown must not wait: pw-cat often ignores SIGTERM while stdin
    # is open, and wait(timeout=1) would delay the first speech samples.
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    _close_stdin(proc)


def _feed(proc, samples):
    pcm = (np.clip(np.asarray(samples), -1.0, 1.0) * 32767).astype(np.int16)

    # Fed from a thread, not written inline here: a multi-second utterance
    # exceeds the OS pipe buffer, so a synchronous write would block the
    # caller until the player drains it — defeating overlap of this
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


def _close_stdin(proc):
    try:
        proc.stdin.close()
    except OSError:
        pass


class Player:
    def __init__(self, cfg: dict):
        self._cfg = cfg
        self._lock = threading.Lock()
        self._stopped = False
        self._proc = None
        self._wake_proc = None
        self._wake_alive = False
        self._wake_gen = 0
        self._kick_thread = None
        self._feeder = None

    def begin(self):
        with self._lock:
            if self._stopped:
                return
            self._wake_alive = True
            gen = self._wake_gen
            kick = threading.Thread(
                target=self._kick_sink,
                args=(gen,),
                daemon=True,
                name="omatalk-wake",
            )
            self._kick_thread = kick
            kick.start()

    def play(self, samples, rate) -> bool:
        feeder = None
        with self._lock:
            if self._stopped:
                return True
            feeder = self._feeder
            self._feeder = None
        if feeder is not None:
            feeder.join()
        started = False
        with self._lock:
            if self._stopped:
                return True
            if self._proc is None:
                try:
                    proc = _start(self._cfg, rate)
                except OSError:
                    return False
                self._proc = proc
                started = True
            elif self._proc.poll() is not None:
                return False
        with self._lock:
            if self._stopped:
                return True
            proc = self._proc
            if proc is None or proc.poll() is not None:
                return False
        feeder = _feed(proc, samples)
        with self._lock:
            if self._stopped:
                return True
            self._feeder = feeder
        # After the first feed: wake already ran in parallel with synthesize.
        # Do not join/wait it before writing speech — that sequenced the
        # shim's death in front of the first word.
        if started:
            self._stop_wake()
        return True

    def finish(self) -> bool:
        with self._lock:
            if self._stopped:
                return True
            proc = self._proc
            feeder = self._feeder
            self._feeder = None
            if proc is None:
                return True
            if proc.poll() is not None:
                return False
        if feeder is not None:
            feeder.join()
        with self._lock:
            if self._stopped:
                return True
            if proc.poll() is not None:
                return False
            _close_stdin(proc)
        proc.wait()
        return True

    def stop(self):
        with self._lock:
            if self._stopped:
                return
            self._stopped = True
            self._wake_alive = False
            self._wake_gen += 1
            wake = self._wake_proc
            self._wake_proc = None
            kick = self._kick_thread
            self._kick_thread = None
            proc = self._proc
            self._proc = None
        _reap(wake)
        _reap(proc)
        if kick is not None and kick.is_alive() and kick is not threading.current_thread():
            kick.join(timeout=2)
        with self._lock:
            leftover = self._wake_proc
            self._wake_proc = None
        _reap(leftover)

    def _kick_sink(self, gen: int):
        try:
            proc = _start(self._cfg, RATE)
            _feed(proc, _wake_pcm()).join()
        except OSError:
            return
        with self._lock:
            if gen != self._wake_gen or not self._wake_alive or self._stopped:
                _reap(proc)
                return
            old = self._wake_proc
            self._wake_proc = proc
        _reap(old)

    def _stop_wake(self):
        with self._lock:
            self._wake_alive = False
            self._wake_gen += 1
            proc = self._wake_proc
            self._wake_proc = None
            kick = self._kick_thread
            self._kick_thread = None
        _drop(proc)
        if kick is not None and kick.is_alive() and kick is not threading.current_thread():
            kick.join(timeout=2)
        with self._lock:
            leftover = self._wake_proc
            self._wake_proc = None
        _drop(leftover)
