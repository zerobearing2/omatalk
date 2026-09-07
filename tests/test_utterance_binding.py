import threading
import time

import pytest

from conftest import FAKES
from daemon.cli import build_parser
from daemon.omatalkd import Daemon, handle, parse_speak, speak_line


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


def play_log(binding_env):
    return (binding_env.parent / "play.log").read_text()


def wait_play_log(binding_env, prefix, count=1, timeout=2):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        lines = [l for l in play_log(binding_env).splitlines() if l.startswith(prefix)]
        if len(lines) >= count:
            return lines
        time.sleep(0.01)
    raise AssertionError(
        f"play.log never reached {count} {prefix!r} lines: {play_log(binding_env)!r}"
    )


def write_config(path, *, voice="af_heart", speed=1.0, lang="en-us", player=None):
    player = player or f"{FAKES}/player"
    path.write_text(
        f'voice = "{voice}"\n'
        f"speed = {speed}\n"
        f'lang = "{lang}"\n'
        f'capture_primary = ["{FAKES}/capture-primary"]\n'
        f'capture_clipboard = ["{FAKES}/capture-clipboard"]\n'
        f'player = ["{player}"]\n'
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
    monkeypatch.setenv("OMATALK_TEST_CAPTURE_FILE", str(tmp_path / "capture.txt"))
    monkeypatch.setenv("OMATALK_TEST_CLIPBOARD_FILE", str(tmp_path / "clipboard.txt"))
    (tmp_path / "play.log").write_text("")
    (tmp_path / "notify.log").write_text("")
    (tmp_path / "capture.txt").write_text("")
    (tmp_path / "clipboard.txt").write_text("")
    # Long enough that a one-chunk Utterance is still alive at finish()
    # after begin() has already started (and may reap) the wake shim.
    (tmp_path / "ticks.txt").write_text("10")
    return config


def set_selection(binding_env, text):
    (binding_env.parent / "capture.txt").write_text(text)


def set_clipboard(binding_env, text):
    (binding_env.parent / "clipboard.txt").write_text(text)


def notify_log(binding_env):
    return (binding_env.parent / "notify.log").read_text()


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


@pytest.mark.parametrize(
    "text, voice, line, parsed",
    [
        ("", None, "speak", (None, "")),
        ("Hi", None, "speak Hi", (None, "Hi")),
        ("", "af_bella", "speak --voice af_bella", ("af_bella", "")),
        (
            "Hi, I'm bella.",
            "af_bella",
            "speak --voice af_bella Hi, I'm bella.",
            ("af_bella", "Hi, I'm bella."),
        ),
    ],
)
def test_speak_line_round_trip(text, voice, line, parsed):
    assert speak_line(text, voice) == line
    cmd, _, payload = line.partition(" ")
    assert cmd == "speak"
    assert parse_speak(payload) == parsed


def test_parse_speak_empty_name_is_not_an_override():
    assert parse_speak("--voice  hi") == (None, "hi")


def test_parse_speak_without_space_is_text():
    assert parse_speak("--voice") == (None, "--voice")


def test_handle_speak_voice_prefix_binds_override(binding_env):
    engine = RecordingEngine()
    daemon = Daemon(engine)
    assert handle(daemon, "speak --voice af_bella Hi, I'm bella.") == "ok"
    wait_state(daemon, "idle")

    assert engine.calls == [("Hi, I'm bella.", "af_bella", 1.0, "en-us")]


def test_cli_speak_voice_encodes_and_handle_binds_without_writing_config(binding_env):
    args = build_parser().parse_args(["speak", "--voice", "af_bella", "Hi,", "I'm", "bella."])
    line = speak_line(" ".join(args.text), args.voice)
    before = binding_env.read_text()

    engine = RecordingEngine()
    daemon = Daemon(engine)
    assert handle(daemon, line) == "ok"
    wait_state(daemon, "idle")

    assert engine.calls == [("Hi, I'm bella.", "af_bella", 1.0, "en-us")]
    assert binding_env.read_text() == before


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


def test_wake_starts_without_waiting_for_synthesize(binding_env):
    started = threading.Event()

    class HoldSynth(RecordingEngine):
        def synthesize(self, text, voice, speed, lang):
            started.set()
            self._release.wait(timeout=10)
            return super().synthesize(text, voice, speed, lang)

    engine = HoldSynth()
    engine.hold_after_first()
    daemon = Daemon(engine)
    daemon.speak("One sentence.")
    assert started.wait(timeout=10)
    wait_play_log(binding_env, "start")
    engine.release()
    wait_state(daemon, "idle")


def test_in_flight_stream_keeps_bound_voice_speed_lang(binding_env):
    engine = RecordingEngine()
    engine.hold_after_first()
    daemon = Daemon(engine)
    first = "A" * 90 + "."
    second = "B" * 90 + "."
    daemon.speak(first + " " + second)
    assert engine._first.wait(timeout=10)

    write_config(binding_env, voice="af_bella", speed=1.5, lang="en-gb")
    engine.release()
    wait_state(daemon, "idle")

    voices = {(voice, speed, lang) for _, voice, speed, lang in engine.calls}
    assert voices == {("af_heart", 1.0, "en-us")}
    assert [text for text, *_ in engine.calls] == [first, second]


def two_windows():
    return "A" * 90 + ".", "B" * 90 + "."


def test_player_exit_mid_utterance_sets_error(binding_env, tmp_path):
    player = tmp_path / "exit-now"
    player.write_text(
        "#!/bin/sh\n"
        'echo "start $$" >> "$OMATALK_TEST_LOG"\n'
        "dd bs=1 count=1 of=/dev/null 2>/dev/null\n"
        'echo "gone $$" >> "$OMATALK_TEST_LOG"\n'
        "exit 0\n"
    )
    player.chmod(0o755)
    write_config(binding_env, player=str(player))

    class WaitForSpeechDeath(RecordingEngine):
        def synthesize(self, text, voice, speed, lang):
            if self.calls:
                # Speech logs gone after reading one byte and exiting; wait
                # so the next play() observes a dead process, not a shell
                # that has not yet reached `exit`.
                wait_play_log(binding_env, "gone")
            return super().synthesize(text, voice, speed, lang)

    first, second = two_windows()
    daemon = Daemon(WaitForSpeechDeath())
    daemon.speak(first + " " + second)
    wait_state(daemon, "error")
    assert "error: player exited" in (binding_env.parent / "notify.log").read_text()


def test_first_chunk_error_reaps_wake(binding_env):
    class BoomFirst(RecordingEngine):
        def synthesize(self, text, voice, speed, lang):
            wait_play_log(binding_env, "start")
            raise RuntimeError("boom")

    daemon = Daemon(BoomFirst())
    daemon.speak("One sentence.")
    wait_state(daemon, "error")
    wait_play_log(binding_env, "killed")
    assert "error: boom" in (binding_env.parent / "notify.log").read_text()


def test_synthesize_error_reaps_player(binding_env):
    class Boom(RecordingEngine):
        def synthesize(self, text, voice, speed, lang):
            self.calls.append((text, voice, speed, lang))
            if len(self.calls) > 1:
                raise RuntimeError("boom")
            return [0.0] * 2400, 24000

    first, second = two_windows()
    daemon = Daemon(Boom())
    daemon.speak(first + " " + second)
    wait_state(daemon, "error")
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        entries = (binding_env.parent / "play.log").read_text().splitlines()
        if any(line.startswith("killed") for line in entries):
            break
        time.sleep(0.05)
    else:
        entries = (binding_env.parent / "play.log").read_text().splitlines()
        raise AssertionError(f"player was not reaped, log={entries!r}")
    assert "error: boom" in (binding_env.parent / "notify.log").read_text()


def test_speak_empty_uses_selection(binding_env):
    set_selection(binding_env, "From the selection.")
    engine = RecordingEngine()
    daemon = Daemon(engine)
    daemon.speak("")
    wait_state(daemon, "idle")

    assert engine.calls == [("From the selection.", "af_heart", 1.0, "en-us")]


def test_speak_empty_falls_back_to_clipboard_when_idle(binding_env):
    set_selection(binding_env, "")
    set_clipboard(binding_env, "From the clipboard instead.")
    engine = RecordingEngine()
    daemon = Daemon(engine)
    daemon.speak("")
    wait_state(daemon, "idle")

    assert engine.calls == [("From the clipboard instead.", "af_heart", 1.0, "en-us")]


def test_speak_empty_notifies_when_source_is_empty(binding_env):
    engine = RecordingEngine()
    daemon = Daemon(engine)
    daemon.speak("")

    assert daemon.state == "idle"
    assert engine.calls == []
    assert "nothing to read" in notify_log(binding_env)


def test_speak_empty_survives_missing_notify_binary(binding_env):
    binding_env.write_text(
        "\n".join(
            'notify = ["/no-such-omatalk-notify"]'
            if line.startswith("notify ")
            else line
            for line in binding_env.read_text().splitlines()
        )
        + "\n"
    )
    engine = RecordingEngine()
    daemon = Daemon(engine)
    daemon.speak("")

    assert daemon.state == "idle"
    assert engine.calls == []


def test_inline_text_skips_selection(binding_env):
    set_selection(binding_env, "Ignored selection.")
    engine = RecordingEngine()
    daemon = Daemon(engine)
    daemon.speak("Only inline text.")
    wait_state(daemon, "idle")

    assert engine.calls == [("Only inline text.", "af_heart", 1.0, "en-us")]
