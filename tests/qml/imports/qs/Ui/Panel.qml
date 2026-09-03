import QtQuick

Item {
  property var bar: null
  property string moduleName: ""
  property var anchorItem: null
  property bool daemonUnavailable: false
  property bool opened: false

  function open() { opened = true }
  function close() { opened = false }
  function toggle() { opened = !opened }
  function switchPanel(direction) { return false }
}
