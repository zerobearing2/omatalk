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
    # SIGTERM first so Interrupt does not drain the pipe. Then EOF: pw-cat
    # often ignores TERM while stdin is open, and wait() can then succeed.
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    _close_stdin(proc)
    try:
        proc.wait(timeout=1)
    except subprocess.TimeoutExpired:
        proc.kill()
        try:
            proc.wait(timeout=1)
        except subprocess.TimeoutExpired:
            pass


def _drop(proc):
    # No wait: first play must not block on the shim.
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    _close_stdin(proc)


def _write_pcm(proc, samples):
    pcm = (np.clip(np.asarray(samples), -1.0, 1.0) * 32767).astype(np.int16)
    try:
        proc.stdin.write(pcm.tobytes())
        proc.stdin.flush()
    except (BrokenPipeError, OSError):
        pass


def _feed(proc, samples):
    # Fed from a thread, not written inline here: a multi-second utterance
    # exceeds the OS pipe buffer, so a synchronous write would block the
    # caller until the player drains it — defeating overlap of this
    # sentence's playback with the next sentence's synthesis. stdin stays
    # open so later sentences can append to the same player process.
    thread = threading.Thread(target=_write_pcm, args=(proc, samples), daemon=True)
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
        self._dropped = []

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
        # First samples into the pipe before any wake teardown.
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
            dropped = self._dropped
            self._dropped = []
        _reap(wake)
        _reap(proc)
        for stale in dropped:
            _reap(stale)
        if kick is not None and kick.is_alive() and kick is not threading.current_thread():
            kick.join(timeout=2)
        with self._lock:
            leftover = self._wake_proc
            self._wake_proc = None
            more = self._dropped
            self._dropped = []
        _reap(leftover)
        for stale in more:
            _reap(stale)

    def _kick_sink(self, gen: int):
        try:
            proc = _start(self._cfg, RATE)
        except OSError:
            return
        # Register and write the wake silence in one critical section: this
        # is the same lock _stop_wake() takes to bump _wake_gen, so the two
        # can never interleave. Either _stop_wake() already ran (stale here,
        # so the wake payload is never written to a proc real speech might
        # race past) or it hasn't yet (so it will find this proc as
        # self._wake_proc, already fully written, and tear it down below).
        # A prior version wrote the silence on a joined thread *before* this
        # check, leaving a window where a concurrent _stop_wake() found no
        # self._wake_proc yet to cancel — the write always completed anyway,
        # sometimes landing after real speech had already started.
        with self._lock:
            if gen != self._wake_gen or not self._wake_alive or self._stopped:
                self._dropped.append(proc)
                stale = True
            else:
                old = self._wake_proc
                self._wake_proc = proc
                stale = False
                _write_pcm(proc, _wake_pcm())
        if stale:
            _drop(proc)
            return
        self._stash(old)
        _drop(old)

    def _stop_wake(self):
        with self._lock:
            self._wake_alive = False
            self._wake_gen += 1
            proc = self._wake_proc
            self._wake_proc = None
        self._stash(proc)
        _drop(proc)
        with self._lock:
            leftover = self._wake_proc
            self._wake_proc = None
        self._stash(leftover)
        _drop(leftover)

    def _stash(self, proc):
        if proc is None:
            return
        with self._lock:
            self._dropped.append(proc)
