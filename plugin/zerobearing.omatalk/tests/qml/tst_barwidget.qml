import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

TestCase {
  name: "OmatalkBarWidget"

  Loader {
    id: loader
    source: Qt.resolvedUrl("../../BarWidget.qml")
  }

  property var widget: loader.item
  readonly property string usrBin: "/usr/bin/omatalk"

  function init() {
    Quickshell.envValues = { "XDG_RUNTIME_DIR": "/tmp" }
    loader.active = false
    loader.active = true
    tryCompare(loader, "status", Loader.Ready)
    verify(widget !== null)
  }

  function runningProbes() {
    var found = []
    for (var i = 0; i < ProcessRegistry.processes.length; i++) {
      var process = ProcessRegistry.processes[i]
      if (process.running && process.command[0] === "test") found.push(process)
    }
    return found
  }

  function awaitProbe() {
    tryVerify(function() { return runningProbes().length > 0 })
    return runningProbes()[0]
  }

  function test_only_usr_bin_is_probed() {
    var probe = awaitProbe()
    compare(probe.command.length, 3)
    compare(probe.command[2], usrBin)
    compare(widget.launcherPath, usrBin)
  }

  function test_the_package_binary_marks_the_daemon_installed() {
    awaitProbe().complete(0, "", "")
    compare(widget.daemonInstalled, true)
    compare(runningProbes().length, 0)
  }

  function test_a_missing_package_binary_stays_uninstalled_and_only_it_is_probed_again() {
    awaitProbe().complete(1, "", "")
    compare(widget.daemonInstalled, false)
    tryVerify(function() { return runningProbes().length > 0 })
    compare(runningProbes()[0].command[2], usrBin)
  }

  function test_the_panel_uses_the_package_binary() {
    awaitProbe().complete(0, "", "")
    compare(widget.panelItem.launcherPath, usrBin)
    compare(widget.panelItem.daemonInstalled, true)
  }
}
