pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui

BarWidget {
  id: root

  moduleName: "zerobearing.omatalk"
  property string daemonState: "idle"
  readonly property bool speaking: daemonState === "speaking"
  readonly property string socketPath: {
    var override = Quickshell.env("OMATALK_SOCKET")
    if (override !== "") return override
    return Quickshell.env("XDG_RUNTIME_DIR") + "/omatalk/omatalk.sock"
  }

  function applyState(raw) {
    var next = String(raw).trim()
    daemonState = next === "idle" || next === "speaking" || next === "error"
      ? next : "idle"
  }

  function scheduleReconnect() {
    daemonState = "idle"
    if (!retry.running) retry.start()
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "󰃦"
    active: root.speaking
    useActiveColor: true
    pressable: false
    tooltipText: root.speaking ? "Omatalk is speaking" : "Omatalk"
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
          root.daemonState = "idle"
          write("follow\n")
          flush()
        } else {
          root.scheduleReconnect()
        }
      }

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
    repeat: false
    onTriggered: {
      stateSocketLoader.active = false
      reconnect.start()
    }
  }

  Timer {
    id: reconnect
    interval: 0
    repeat: false
    onTriggered: stateSocketLoader.active = true
  }
}
