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

  function test_sample_and_snap() {
    compare(panel.sampleTextFor("af_bella"), "Hi, I'm bella. This is what I sound like.")
    compare(panel.snapSpeed(1.73), 1.7)
    verify(panel.isEnglishVoice("bf_emma"))
    verify(!panel.isEnglishVoice("jf_alpha"))
  }

  function test_setup_points_at_the_site_install_page() {
    compare(panel.installUrl, "https://omatalk.zerobearing.com/#install")
    verify(panel.showingSetup)
  }

  function test_open_without_launcher_shows_setup_and_skips_config_cli() {
    compare(panel.daemonInstalled, false)
    verify(panel.showingSetup)
    panel.opened = true
    compare(findProc("omatalk version").running, false)
    compare(findProc("config get --json").running, false)
    compare(findProc("config voices --json").running, false)
  }

  function test_open_install_page_launches_the_browser() {
    panel.openInstallPage()
    var open = findProc("xdg-open")
    verify(open.running)
    compare(open.command[1], panel.installUrl)
  }

  function test_launcher_appearing_refreshes_config() {
    panel.opened = true
    panel.daemonInstalled = true
    verify(findProc("omatalk version").running)
    verify(findProc("config get --json").running)
    compare(panel.showingSetup, false)
  }

  function test_refresh_fills_config_and_version() {
    panel.daemonInstalled = true
    compare(panel.daemonVersion, "unknown")
    panel.refresh()
    findProc("config voices --json").complete(0, '["af_heart","jf_skip","am_test"]', "")
    findProc("config get --json").complete(0, '{"voice":"af_heart","speed":1.25}', "")
    findProc("omatalk version").complete(0, "0.2.1-test\n", "")
    compare(panel.voiceOptions, ["af_heart", "am_test"])
    compare(panel.voice, "af_heart")
    compare(panel.speed, 1.25)
    compare(panel.daemonVersion, "0.2.1-test")
    compare(panel.versionLabel, "omatalk 0.2.1-test")
  }

  function test_installed_panel_shows_only_the_daemon_version() {
    panel.daemonInstalled = true
    panel.daemonVersion = "0.2.1-test"
    compare(panel.versionLabel, "omatalk 0.2.1-test")
    compare(panel.showingSetup, false)
  }

  function test_failed_version_is_unknown() {
    panel.daemonInstalled = true
    panel.refresh()
    findProc("omatalk version").complete(1, "", "nope")
    compare(panel.daemonVersion, "unknown")
    compare(panel.versionLabel, "omatalk unknown")
  }

  function test_set_voice_saves_and_previews() {
    panel.daemonInstalled = true
    panel.setVoice("bf_emma")
    compare(commandLine(findProc("config set voice")), launcherBin + " config set voice bf_emma")
    compare(
      commandLine(findProc("speak --voice")),
      launcherBin + " speak --voice bf_emma Hi, I'm emma. This is what I sound like."
    )
  }

  function test_set_speed_saves_snapped_value() {
    panel.daemonInstalled = true
    panel.setSpeed(panel.snapSpeed(1.73))
    compare(commandLine(findProc("config set speed")), launcherBin + " config set speed 1.7")
  }

  function test_open_refreshes() {
    panel.daemonInstalled = true
    panel.opened = true
    verify(findProc("omatalk version").running)
  }
}
