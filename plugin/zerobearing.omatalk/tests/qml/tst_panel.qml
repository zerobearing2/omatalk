import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

TestCase {
  name: "OmatalkPanel"

  Loader {
    id: loader
    source: Qt.resolvedUrl("../../Panel.qml")
  }

  property var panel: loader.item
  readonly property string launcherBin: "/usr/bin/omatalk"

  function init() {
    Quickshell.envValues = { "XDG_RUNTIME_DIR": "/tmp" }
    loader.active = false
    loader.active = true
    tryCompare(loader, "status", Loader.Ready)
    verify(panel !== null)
  }

  function commandLine(process) {
    var parts = []
    for (var i = 0; i < process.command.length; i++) parts.push(String(process.command[i]))
    return parts.join(" ")
  }

  function findProc(needle) {
    for (var i = 0; i < ProcessRegistry.processes.length; i++) {
      var process = ProcessRegistry.processes[i]
      if (commandLine(process).indexOf(needle) !== -1) return process
    }
    return null
  }

  function hero() { return findChild(panel, "omatalkHero") }

  function test_sample_and_snap() {
    compare(panel.sampleTextFor("af_bella"), "Hi, I'm bella. This is what I sound like.")
    compare(panel.snapSpeed(1.73), 1.7)
    compare(panel.snapWakeLead(626), 650)
    compare(panel.snapWakeLead(624), 600)
    compare(panel.snapWakeLead(-30), 0)
    compare(panel.snapWakeLead(2030), 2000)
    compare(panel.wakeLeadLabel(0), "Off")
    compare(panel.wakeLeadLabel(650), "650 ms")
    verify(panel.isEnglishVoice("bf_emma"))
    verify(!panel.isEnglishVoice("jf_alpha"))
  }


  function test_refresh_fills_config_and_version() {
    compare(panel.daemonVersion, "unknown")
    panel.refresh()
    findProc("config voices --json").complete(0, '["af_heart","jf_skip","am_test"]', "")
    findProc("config get --json").complete(0, '{"voice":"af_heart","speed":1.25}', "")
    findProc("omatalk version").complete(0, "0.2.1-test\n", "")
    compare(panel.voiceOptions, ["af_heart", "am_test"])
    compare(panel.voice, "af_heart")
    compare(panel.speed, 1.25)
    compare(panel.daemonVersion, "0.2.1-test")
    compare(hero().detail, "0.2.1-test")
  }

  function test_refresh_fills_media_pause_and_wake_lead() {
    compare(panel.pauseMedia, true)
    compare(panel.wakeLeadMs, 650)
    panel.refresh()
    findProc("config get --json").complete(
      0, '{"voice":"af_heart","speed":1.0,"pause_media":false,"wake_lead_ms":0}', "")
    compare(panel.pauseMedia, false)
    compare(panel.wakeLeadMs, 0)
  }

  function test_get_without_the_new_keys_keeps_the_defaults() {
    panel.refresh()
    findProc("config get --json").complete(0, '{"voice":"af_heart","speed":1.0}', "")
    compare(panel.pauseMedia, true)
    compare(panel.wakeLeadMs, 650)
  }

  function test_failed_version_is_unknown() {
    panel.refresh()
    findProc("omatalk version").complete(1, "", "nope")
    compare(panel.daemonVersion, "unknown")
    compare(hero().detail, "unknown")
  }

  function test_set_voice_saves_and_previews() {
    panel.setVoice("bf_emma")
    compare(commandLine(findProc("config set voice")), launcherBin + " config set voice bf_emma")
    compare(
      commandLine(findProc("speak --voice")),
      launcherBin + " speak --voice bf_emma Hi, I'm emma. This is what I sound like."
    )
  }

  function test_set_speed_saves_snapped_value() {
    panel.setSpeed(panel.snapSpeed(1.73))
    compare(commandLine(findProc("config set speed")), launcherBin + " config set speed 1.7")
  }

  function test_set_pause_media_saves_the_flipped_value() {
    panel.setPauseMedia(false)
    compare(panel.pauseMedia, false)
    compare(commandLine(findProc("config set pause_media")), launcherBin + " config set pause_media false")
    panel.setPauseMedia(true)
    compare(commandLine(findProc("config set pause_media")), launcherBin + " config set pause_media true")
  }

  function test_set_wake_lead_saves_snapped_value() {
    panel.setWakeLead(panel.snapWakeLead(1230))
    compare(commandLine(findProc("config set wake_lead_ms")), launcherBin + " config set wake_lead_ms 1250")
    panel.setWakeLead(panel.snapWakeLead(0))
    compare(commandLine(findProc("config set wake_lead_ms")), launcherBin + " config set wake_lead_ms 0")
  }

  function test_set_errors_show_and_clear() {
    panel.setPauseMedia(false)
    findProc("config set pause_media").complete(1, "", "false: nope\n")
    compare(panel.pauseMediaError, "false: nope")
    panel.setPauseMedia(true)
    findProc("config set pause_media").complete(0, "", "")
    compare(panel.pauseMediaError, "")

    panel.setWakeLead(650)
    findProc("config set wake_lead_ms").complete(1, "", "bad\n")
    compare(panel.wakeLeadError, "bad")
    panel.setWakeLead(700)
    findProc("config set wake_lead_ms").complete(0, "", "")
    compare(panel.wakeLeadError, "")
  }

  function test_controls_drive_the_setters() {
    var toggle = findChild(panel, "omatalkPauseMediaToggle")
    var slider = findChild(panel, "omatalkWakeLeadSlider")
    verify(toggle !== null)
    verify(slider !== null)
    compare(toggle.label, "Pause media while speaking")

    toggle.clicked()
    compare(commandLine(findProc("config set pause_media")), launcherBin + " config set pause_media false")
    compare(toggle.checked, false)

    slider.released(1233)
    compare(commandLine(findProc("config set wake_lead_ms")), launcherBin + " config set wake_lead_ms 1250")
    compare(slider.maximum, 2000)
    compare(slider.step, 50)
  }

  function test_controls_show_what_get_returned() {
    panel.refresh()
    findProc("config get --json").complete(
      0, '{"voice":"af_heart","speed":1.0,"pause_media":false,"wake_lead_ms":0}', "")
    compare(findChild(panel, "omatalkPauseMediaToggle").checked, false)
    compare(findChild(panel, "omatalkWakeLeadSlider").value, 0)
    compare(findChild(panel, "omatalkWakeLeadLabel").text, "Off")
    findProc("config get --json").complete(
      0, '{"voice":"af_heart","speed":1.0,"pause_media":true,"wake_lead_ms":650}', "")
    compare(findChild(panel, "omatalkPauseMediaToggle").checked, true)
    compare(findChild(panel, "omatalkWakeLeadLabel").text, "650 ms")
  }

  function test_hero_icon_is_the_injected_glyph() {
    panel.glyph = "G"
    compare(hero().iconItem.text, "G")
  }

  function test_status_line_follows_the_daemon_state() {
    compare(panel.statusLine, "Local TTS · F8 to speak")
    compare(hero().meta, panel.statusLine)

    panel.daemonState = "speaking"
    compare(hero().meta, "Speaking…")

    panel.daemonUnavailable = true
    compare(hero().meta, "Daemon not running")

    panel.daemonState = "idle"
    compare(hero().meta, "Daemon not running")

    panel.daemonUnavailable = false
    compare(hero().meta, "Local TTS · F8 to speak")
  }

  function test_help_lines_sit_under_their_controls() {
    compare(
      findChild(panel, "omatalkPauseMediaToggle").description,
      "Spotify, browsers and video pause, then resume when omatalk finishes.")
    compare(
      findChild(panel, "omatalkWakeLeadHelp").text,
      "Silence before speech so headphones don't clip the first word.")
  }

  function test_preview_button_speaks_the_selected_voice_without_saving() {
    var button = findChild(panel, "omatalkPreviewButton")
    verify(button !== null)
    compare(button.enabled, false)

    panel.voice = "bf_emma"
    compare(button.enabled, true)
    button.clicked()
    var speak = findProc("speak --voice")
    verify(speak.running)
    compare(
      commandLine(speak),
      launcherBin + " speak --voice bf_emma Hi, I'm emma. This is what I sound like."
    )
    compare(findProc("config set voice"), null)
  }

  function test_open_refreshes() {
    panel.opened = true
    verify(findProc("omatalk version").running)
  }
}
