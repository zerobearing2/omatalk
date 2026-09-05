import threading
import time

import pytest

from conftest import FAKES
from daemon.omatalkd import Daemon, handle


class RecordingEngine:
    def __init__(self):
        self.calls = []
        self._first = threading.Event()
        self._release = threading.Event()
        self._release.set()

    def hold_after_first(self):
        self._release.clear()

    def release(self):
        self._release.set()

    def synthesize(self, text: str, voice: str, speed: float, lang: str):
        self.calls.append((text, voice, speed, lang))
        if len(self.calls) == 1:
            self._first.set()
            self._release.wait(timeout=10)
        return [0.0] * 2400, 24000


def wait_state(daemon, want, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if daemon.state == want:
            return
        time.sleep(0.05)
    raise AssertionError(f"state never reached {want!r}, last {daemon.state!r}")


def write_config(path, *, voice="af_heart", speed=1.0, lang="en-us"):
    path.write_text(
        f'voice = "{voice}"\n'
        f"speed = {speed}\n"
        f'lang = "{lang}"\n'
        f'capture_primary = ["{FAKES}/capture-primary"]\n'
        f'capture_clipboard = ["{FAKES}/capture-clipboard"]\n'
        f'player = ["{FAKES}/player"]\n'
        f'notify = ["{FAKES}/notify"]\n'
    )


@pytest.fixture
def binding_env(tmp_path, monkeypatch):
    config = tmp_path / "config.toml"
    write_config(config)
    monkeypatch.setenv("OMATALK_CONFIG", str(config))
    monkeypatch.setenv("OMATALK_TEST_LOG", str(tmp_path / "play.log"))
    monkeypatch.setenv("OMATALK_TEST_NOTIFY_LOG", str(tmp_path / "notify.log"))
    monkeypatch.setenv("OMATALK_TEST_TICKS_FILE", str(tmp_path / "ticks.txt"))
    (tmp_path / "play.log").write_text("")
    (tmp_path / "ticks.txt").write_text("1")
    return config


def test_speak_binds_configured_voice_speed_lang(binding_env):
    engine = RecordingEngine()
    daemon = Daemon(engine)
    daemon.speak("One sentence.")
    wait_state(daemon, "idle")

    assert engine.calls == [("One sentence.", "af_heart", 1.0, "en-us")]


def test_speak_voice_override_binds_override_and_configured_speed(binding_env):
    engine = RecordingEngine()
    daemon = Daemon(engine)
    daemon.speak("Hi, I'm bella.", voice="af_bella")
    wait_state(daemon, "idle")

    assert engine.calls == [("Hi, I'm bella.", "af_bella", 1.0, "en-us")]


def test_handle_speak_voice_prefix_binds_override(binding_env):
    engine = RecordingEngine()
    daemon = Daemon(engine)
    assert handle(daemon, "speak --voice af_bella Hi, I'm bella.") == "ok"
    wait_state(daemon, "idle")

    assert engine.calls == [("Hi, I'm bella.", "af_bella", 1.0, "en-us")]


def test_back_to_back_overrides_bind_distinct_voices(binding_env):
    engine = RecordingEngine()
    daemon = Daemon(engine)
    daemon.speak("Hi, I'm bella.", voice="af_bella")
    wait_state(daemon, "idle")
    daemon.speak("Hi, I'm george.", voice="bm_george")
    wait_state(daemon, "idle")

    voices = [voice for _, voice, _, _ in engine.calls]
    assert voices == ["af_bella", "bm_george"]


def test_next_utterance_binds_new_config(binding_env):
    engine = RecordingEngine()
    daemon = Daemon(engine)
    daemon.speak("First utterance.")
    wait_state(daemon, "idle")

    write_config(binding_env, voice="af_bella", speed=1.5, lang="en-gb")
    daemon.speak("Second utterance.")
    wait_state(daemon, "idle")

    assert engine.calls[0] == ("First utterance.", "af_heart", 1.0, "en-us")
    assert engine.calls[1] == ("Second utterance.", "af_bella", 1.5, "en-gb")


def test_in_flight_stream_keeps_bound_voice_speed_lang(binding_env):
    engine = RecordingEngine()
    engine.hold_after_first()
    daemon = Daemon(engine)
    daemon.speak("One sentence. Two sentence.")
    assert engine._first.wait(timeout=10)

    write_config(binding_env, voice="af_bella", speed=1.5, lang="en-gb")
    engine.release()
    wait_state(daemon, "idle")

    voices = {(voice, speed, lang) for _, voice, speed, lang in engine.calls}
    assert voices == {("af_heart", 1.0, "en-us")}
    assert len(engine.calls) == 2
