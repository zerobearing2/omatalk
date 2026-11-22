import QtQuick
import QtTest
import Quickshell

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

  function test_the_panel_uses_the_package_binary() {
    compare(widget.panelItem.launcherPath, usrBin)
  }

  function test_a_daemon_error_marks_the_widget_and_panel_unavailable() {
    compare(widget.daemonUnavailable, false)
    widget.applyState("error\n")
    compare(widget.daemonUnavailable, true)
    compare(widget.panelItem.daemonUnavailable, true)
  }
}
