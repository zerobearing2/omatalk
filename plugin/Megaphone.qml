pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui

BarWidget {
  id: root

  moduleName: "zerobearing.omatalk"
  property string daemonState: "idle"
  readonly property string socketOverride: Quickshell.env("OMATALK_SOCKET")
  readonly property string runtimeDir: Quickshell.env("XDG_RUNTIME_DIR")
  property string userId: ""
  readonly property bool speaking: daemonState === "speaking"
  readonly property string socketPath: {
    if (socketOverride !== "") return socketOverride
    if (runtimeDir !== "") return runtimeDir + "/omatalk/omatalk.sock"
    if (userId !== "") return "/run/user/" + userId + "/omatalk/omatalk.sock"
    return ""
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

  Process {
    id: userIdProcess
    command: ["id", "-u"]
    running: root.socketOverride === "" && root.runtimeDir === ""
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.userId = String(text).trim()
    }
  }

  Component {
    id: stateSocketComponent

    Socket {
      id: stateSocket
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
