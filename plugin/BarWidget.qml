pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

BarWidget {
  id: root

  moduleName: "zerobearing.omatalk"
  property string daemonState: "idle"
  property bool connectionLost: false
  readonly property string socketOverride: Quickshell.env("OMATALK_SOCKET")
  readonly property string runtimeDir: Quickshell.env("XDG_RUNTIME_DIR")
  readonly property bool speaking: daemonState === "speaking"
  readonly property bool daemonUnavailable: daemonState === "error"
    || (connectionLost && !reconnectGrace.running)
  readonly property string socketPath: {
    if (socketOverride !== "") return socketOverride
    if (runtimeDir !== "") return runtimeDir + "/omatalk/omatalk.sock"
    return ""
  }

  function applyState(raw) {
    var next = String(raw).trim()
    daemonState = next === "idle" || next === "speaking" || next === "error"
      ? next : "idle"
  }

  function scheduleReconnect() {
    daemonState = "idle"
    connectionLost = true
    if (!reconnectGrace.running) reconnectGrace.start()
  }

  // Shape contract for Bar.findPanelWidget (requires open/close/opened on
  // the bar-widget root), mirroring the weather plugin's BarWidget/Panel
  // split.
  function injectPanel() {
    var target = panelLoader.item
    if (!target) return
    if ("bar" in target) target.bar = root.bar
    if ("anchorItem" in target) target.anchorItem = button
    if ("daemonUnavailable" in target) target.daemonUnavailable = root.daemonUnavailable
  }

  function togglePanel() {
    if (panelLoader.item && panelLoader.item.toggle) panelLoader.item.toggle()
  }

  readonly property bool opened: panelLoader.item ? panelLoader.item.opened === true : false
  readonly property var panelItem: panelLoader.item

  function open() {
    if (panelLoader.item && panelLoader.item.open) panelLoader.item.open()
  }

  function close() {
    if (panelLoader.item && panelLoader.item.close) panelLoader.item.close()
  }

  onBarChanged: injectPanel()
  onDaemonUnavailableChanged: injectPanel()

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  Loader {
    id: panelLoader
    active: true
    source: Qt.resolvedUrl("Panel.qml")
    onLoaded: {
      root.injectPanel()
      Qt.callLater(root.injectPanel)
    }
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    active: root.speaking || root.daemonUnavailable
    activeColor: root.daemonUnavailable ? Color.urgent : Color.accent
    useActiveColor: true
    tooltipText: root.daemonUnavailable ? "Omatalk is unavailable"
      : (root.speaking ? "Omatalk is speaking" : "Omatalk")
    text: "󰃦"

    onPressed: function(b) { if (b === Qt.LeftButton) root.togglePanel() }
  }

  Component {
    id: stateSocketComponent

    Socket {
      path: root.socketPath
      connected: true
      parser: SplitParser {
        onRead: function(data) { root.applyState(data) }
      }

      onConnectionStateChanged: {
        if (connected) {
          reconnectGrace.stop()
          root.connectionLost = false
          root.daemonState = "idle"
          write("follow\n")
          flush()
        } else {
          root.scheduleReconnect()
        }
      }

      // Inline on purpose: a failed connect can raise the error synchronously
      // while the Loader is still constructing this object, before a
      // Connections element bound to the Loader's item would see it.
      onError: root.scheduleReconnect()
    }
  }

  Loader {
    id: stateSocketLoader
    sourceComponent: stateSocketComponent
    active: true
  }

  Timer {
    id: retry
    interval: 1000
    // Driven by the socket's connected property instead of signal handlers,
    // so a missed signal can never stop the reconnect loop.
    repeat: true
    running: root.socketPath !== ""
      && !(stateSocketLoader.item && stateSocketLoader.item.connected)
    // A Socket whose connect failed cannot redial itself (setConnected(true)
    // is a no-op while its internal QLocalSocket exists), so recreate it.
    onTriggered: {
      stateSocketLoader.active = false
      stateSocketLoader.active = true
    }
  }

  Timer {
    id: reconnectGrace
    interval: 3000
    repeat: false
  }
}
