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
    if (!retry.running) retry.start()
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    active: root.speaking || root.daemonUnavailable
    activeColor: root.daemonUnavailable ? Color.urgent : Color.accent
    useActiveColor: true
    pressable: false
    tooltipText: root.daemonUnavailable ? "Omatalk is unavailable"
      : (root.speaking ? "Omatalk is speaking" : "Omatalk")
    text: "󰃦"
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
          retry.stop()
          reconnectGrace.stop()
          root.connectionLost = false
          root.daemonState = "idle"
          write("follow\n")
          flush()
        } else {
          root.scheduleReconnect()
        }
      }

    }
  }

  Loader {
    id: stateSocketLoader
    sourceComponent: stateSocketComponent
    active: true
  }

  Connections {
    target: stateSocketLoader.item
    function onError() { root.scheduleReconnect() }
  }

  Timer {
    id: retry
    interval: 1000
    repeat: true
    onTriggered: {
      stateSocketLoader.active = false
      reconnect.start()
    }
  }

  Timer {
    id: reconnectGrace
    interval: 3000
    repeat: false
  }

  Timer {
    id: reconnect
    interval: 0
    repeat: false
    onTriggered: stateSocketLoader.active = true
  }
}
