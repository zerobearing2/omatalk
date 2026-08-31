import time

from conftest import (
    FAKES,
    REPO,
    clear_logs,
    log,
    send,
    set_capture,
    set_play_ticks,
    wait_log,
    wait_status,
)


def read_notify(daemon) -> str:
    return (daemon["tmp"] / "notify.log").read_text()


def test_unknown_command(daemon):
    assert send(daemon, "frobnicate") == "unknown command"


def test_status_idle(daemon):
    assert send(daemon, "status") == "idle"


def test_speak_captured_text_plays_all_chunks(daemon):
    clear_logs(daemon)
    set_capture(daemon, "Hello there. Second sentence here. Third one.")
    assert send(daemon, "speak") == "ok"
    wait_status(daemon, "speaking")
    wait_status(daemon, "idle")
    entries = log(daemon).splitlines()
    assert len([l for l in entries if l.startswith("start")]) == 3
    assert len([l for l in entries if l.startswith("end")]) == 3
    assert not [l for l in entries if l.startswith("killed")]


def test_speak_inline_text(daemon):
    clear_logs(daemon)
    assert send(daemon, "speak Only inline text.") == "ok"
    wait_status(daemon, "speaking")
    wait_status(daemon, "idle")
    assert len([l for l in log(daemon).splitlines() if l.startswith("end")]) == 1


def test_interrupt_cuts_current_and_plays_new(daemon):
    clear_logs(daemon)
    set_play_ticks(daemon, "30")
    set_capture(daemon, "First long sentence. Second long sentence.")
    assert send(daemon, "speak") == "ok"
    wait_status(daemon, "speaking")
    wait_log(daemon, "start")
    assert send(daemon, "speak Quick replacement.") == "ok"
    wait_status(daemon, "idle")
    entries = log(daemon).splitlines()
    assert [l for l in entries if l.startswith("killed")]


def test_stop_cuts_playback(daemon):
    clear_logs(daemon)
    set_play_ticks(daemon, "30")
    set_capture(daemon, "A sentence that plays for a while. And another.")
    assert send(daemon, "speak") == "ok"
    wait_status(daemon, "speaking")
    wait_log(daemon, "start")
    assert send(daemon, "stop") == "ok"
    assert send(daemon, "status") == "idle"
    assert [l for l in log(daemon).splitlines() if l.startswith("killed")]


def test_empty_source_notifies_and_stays_idle(daemon):
    clear_logs(daemon)
    set_play_ticks(daemon, "1")
    set_capture(daemon, "")
    before = log(daemon).splitlines()
    assert send(daemon, "speak") == "ok"
    time.sleep(0.5)
    assert send(daemon, "status") == "idle"
    assert "nothing to read" in read_notify(daemon)
    assert log(daemon).splitlines() == before


def test_empty_selection_while_speaking_stops(daemon):
    clear_logs(daemon)
    set_play_ticks(daemon, "30")
    set_capture(daemon, "A sentence that plays for a while. And another.")
    assert send(daemon, "speak") == "ok"
    wait_log(daemon, "start")
    set_capture(daemon, "")
    assert send(daemon, "speak") == "ok"
    assert send(daemon, "status") == "idle"
    wait_log(daemon, "killed")
    assert "nothing to read" not in read_notify(daemon)


def test_same_text_press_while_speaking_stops(daemon):
    clear_logs(daemon)
    set_play_ticks(daemon, "30")
    set_capture(daemon, "Sticky selection text. Still the same text.")
    assert send(daemon, "speak") == "ok"
    wait_log(daemon, "start")
    assert send(daemon, "speak") == "ok"
    assert send(daemon, "status") == "idle"
    wait_log(daemon, "killed")
    wait_log(daemon, "start", count=1)


def test_new_selection_interrupts_and_speaks_new(daemon):
    clear_logs(daemon)
    set_play_ticks(daemon, "30")
    set_capture(daemon, "First very long utterance. Still going on and on.")
    assert send(daemon, "speak") == "ok"
    wait_log(daemon, "start")
    set_capture(daemon, "Completely new selection.")
    assert send(daemon, "speak") == "ok"
    wait_status(daemon, "speaking")
    wait_status(daemon, "idle")
    starts = len([l for l in log(daemon).splitlines() if l.startswith("start")])
    assert starts == 2


def test_playback_failure_notifies_and_daemon_survives(daemon):
    import subprocess

    cfg_path = daemon["tmp"] / "config-fail.toml"
    cfg_path.write_text(
        f'capture_primary = ["{FAKES}/capture-primary"]\n'
        f'capture_clipboard = ["{FAKES}/capture-clipboard"]\n'
        f'player = ["{FAKES}/player-fail"]\n'
        f'notify = ["{FAKES}/notify"]\n'
    )
    env = {
        **daemon["env"],
        "OMATALK_CONFIG": str(cfg_path),
        "OMATALK_SOCKET": str(daemon["tmp"] / "fail.sock"),
    }
    fail_proc = subprocess.Popen([str(REPO / "bin" / "omatalkd")], env=env)
    deadline = time.time() + 60
    while not (daemon["tmp"] / "fail.sock").exists():
        assert time.time() < deadline
        time.sleep(0.1)
    try:
        clear_logs(daemon)
        assert send(daemon, "speak One sentence.", sock="fail.sock") == "ok"
        wait_status(daemon, "error", sock="fail.sock")
        assert "error" in read_notify(daemon)
        assert send(daemon, "stop", sock="fail.sock") == "ok"
        assert send(daemon, "status", sock="fail.sock") == "idle"
    finally:
        fail_proc.terminate()
        fail_proc.wait(timeout=10)


def test_clipboard_fallback_when_idle(daemon):
    clear_logs(daemon)
    set_play_ticks(daemon, "1")
    set_capture(daemon, "")
    (daemon["tmp"] / "clipboard.txt").write_text("From the clipboard instead.")
    assert send(daemon, "speak") == "ok"
    wait_status(daemon, "speaking")
    wait_status(daemon, "idle")
    assert len([l for l in log(daemon).splitlines() if l.startswith("end")]) == 1
