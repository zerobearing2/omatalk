import QtQuick
import QtTest
import Quickshell.Io

TestCase {
  name: "OmatalkPanel"

  Loader {
    id: loader
    source: Qt.resolvedUrl("../../plugin/Panel.qml")
  }

  property var panel: loader.item

  function init() {
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

  function test_sample_and_snap() {
    compare(panel.sampleTextFor("af_bella"), "Hi, I'm bella. This is what I sound like.")
    compare(panel.snapSpeed(1.73), 1.7)
    verify(panel.isEnglishVoice("bf_emma"))
    verify(!panel.isEnglishVoice("jf_alpha"))
  }

  function test_refresh_fills_config_and_version() {
    compare(panel.version, "unknown")
    panel.refresh()
    findProc("config voices --json").complete(0, '["af_heart","jf_skip","am_test"]', "")
    findProc("config get --json").complete(0, '{"voice":"af_heart","speed":1.25}', "")
    findProc("omatalk version").complete(0, "0.2.1-test\n", "")
    compare(panel.voiceOptions, ["af_heart", "am_test"])
    compare(panel.voice, "af_heart")
    compare(panel.speed, 1.25)
    compare(panel.version, "0.2.1-test")
  }

  function test_failed_version_is_unknown() {
    panel.refresh()
    findProc("omatalk version").complete(1, "", "nope")
    compare(panel.version, "unknown")
  }

  function test_set_voice_saves_and_previews() {
    panel.setVoice("bf_emma")
    compare(commandLine(findProc("config set voice")), "omatalk config set voice bf_emma")
    compare(
      commandLine(findProc("speak --voice")),
      "omatalk speak --voice bf_emma Hi, I'm emma. This is what I sound like."
    )
  }

  function test_set_speed_saves_snapped_value() {
    panel.setSpeed(panel.snapSpeed(1.73))
    compare(commandLine(findProc("config set speed")), "omatalk config set speed 1.7")
  }

  function test_open_refreshes() {
    panel.opened = true
    verify(findProc("omatalk version").running)
  }
}
