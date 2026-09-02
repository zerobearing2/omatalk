import QtQuick
import Quickshell.Io
import qs.Commons
import qs.Ui

// Voice + speed config panel. Every control here auto-saves — there is no
// save button — by shelling the `omatalk config` CLI, which owns all
// config.toml reads and writes. The already-running Daemon (if any) picks
// the change up on its own via its own mtime-poll reload; this panel never
// touches config.toml directly and never needs the Daemon to be up.
Panel {
  id: root
  moduleName: "zerobearing.omatalk"

  property var anchorItem: null
  property bool daemonUnavailable: false

  readonly property var englishPrefixes: ["af_", "am_", "bf_", "bm_"]

  // PanelSlider only snaps `step` for wheel nudges — dragging reports
  // continuous precision, snapping is the caller's job per its own docs —
  // so both the live label and the committed value round through this.
  function snapSpeed(v) { return Math.round(v * 10) / 10 }

  // Test hook only: `keyCatcher` lives inside KeyboardPanel's separate
  // PanelWindow surface, so it isn't reachable from outside this document
  // via the normal Item.children tree. Exposing it lets a headless test
  // walk down to the voice dropdown / speed slider by objectName.
  readonly property Item contentRoot: keyCatcher

  property var voiceOptions: []
  property string voice: ""
  property real speed: 1.0
  property string voiceError: ""
  property string speedError: ""
  property string version: "unknown"

  function matchedPrefix(name) {
    for (var i = 0; i < englishPrefixes.length; i++) {
      if (String(name).indexOf(englishPrefixes[i]) === 0) return englishPrefixes[i]
    }
    return null
  }

  function isEnglishVoice(name) {
    return root.matchedPrefix(name) !== null
  }

  // Strips the locale/gender prefix so "af_bella" reads as a name, not a
  // filename, then adds a second sentence — long enough to actually judge
  // the voice's tone by ear, not just prove distinctness between voices.
  // Every option in voiceOptions passed isEnglishVoice, so matchedPrefix
  // always finds one.
  function sampleTextFor(name) {
    var prefix = root.matchedPrefix(name)
    var stripped = prefix !== null ? String(name).slice(prefix.length) : name
    return "Hi, I'm " + stripped + ". This is what I sound like."
  }

  function refresh() {
    voicesProc.running = true
    getProc.running = true
    versionProc.running = true
  }

  onOpenedChanged: if (opened) refresh()

  function setVoice(value) {
    root.voice = value
    setVoiceProc.command = ["omatalk", "config", "set", "voice", value]
    setVoiceProc.running = true
    // Not sequenced after setVoiceProc: the preview never touches
    // config.toml or waits on the Daemon's reload, so there is nothing to
    // wait for — it fires in parallel with the save.
    previewProc.command = ["omatalk", "speak", "--voice", value, root.sampleTextFor(value)]
    previewProc.running = true
  }

  function setSpeed(value) {
    root.speed = value
    setSpeedProc.command = ["omatalk", "config", "set", "speed", String(value)]
    setSpeedProc.running = true
  }

  Process {
    id: voicesProc
    command: ["omatalk", "config", "voices", "--json"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        try {
          var all = JSON.parse(text)
          var english = []
          for (var i = 0; i < all.length; i++) {
            if (root.isEnglishVoice(all[i])) english.push(all[i])
          }
          root.voiceOptions = english
        } catch (e) {
          // Leave the previous option list in place on a bad/empty response.
        }
      }
    }
  }

  Process {
    id: getProc
    command: ["omatalk", "config", "get", "--json"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        try {
          var cfg = JSON.parse(text)
          if (cfg.voice !== undefined) root.voice = cfg.voice
          if (cfg.speed !== undefined) root.speed = cfg.speed
        } catch (e) {
          // Leave the previous values in place on a bad/empty response.
        }
      }
    }
  }

  Process {
    id: setVoiceProc
    stdout: StdioCollector { waitForEnd: true }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.voiceError = text.trim()
    }
    onExited: function(exitCode) { if (exitCode === 0) root.voiceError = "" }
  }

  Process {
    id: previewProc
    stdout: StdioCollector { waitForEnd: true }
    stderr: StdioCollector { waitForEnd: true }
  }

  Process {
    id: setSpeedProc
    stdout: StdioCollector { waitForEnd: true }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.speedError = text.trim()
    }
    onExited: function(exitCode) { if (exitCode === 0) root.speedError = "" }
  }

  Process {
    id: versionProc
    command: ["omatalk", "--version"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var next = text.trim()
        root.version = next !== "" ? next : "unknown"
      }
    }
    // Unlike voices/get, which keep last-known functional state on a bad
    // reply, version is a label. A failed or empty lookup must not keep
    // showing a stale release number — "unknown" is the honest fallback.
    onExited: function(exitCode) { if (exitCode !== 0) root.version = "unknown" }
  }

  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(340))
    contentHeight: panel.fittedContentHeight(content.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }

      Column {
        id: content
        width: parent.width
        spacing: Style.space(14)

        Text {
          text: "Omatalk"
          color: Color.popups.text
          font.family: Style.font.family
          font.pixelSize: Style.font.display
          font.bold: true
        }

        PanelSeparator {}

        PanelSectionHeader { text: "VOICE" }

        SearchableDropdown {
          id: voiceDropdown
          objectName: "omatalkVoiceDropdown"
          width: parent.width
          options: root.voiceOptions
          placeholderText: "Search voices…"
          onChanged: function(v) { root.setVoice(v) }

          Binding on value { value: root.voice }
        }

        Text {
          visible: root.voiceError !== ""
          width: parent.width
          wrapMode: Text.WordWrap
          text: root.voiceError
          color: Color.urgent
          font.family: Style.font.family
          font.pixelSize: Style.font.bodySmall
        }

        PanelSectionHeader { text: "SPEED" }

        Row {
          width: parent.width
          spacing: Style.space(12)

          PanelSlider {
            id: speedSlider
            objectName: "omatalkSpeedSlider"
            bar: root.bar
            width: parent.width - speedLabel.width - Style.space(12)
            minimum: 0.5
            maximum: 2.0
            step: 0.1
            value: root.speed
            tickCount: 16
            tickColor: Color.popups.background
            onReleased: function(v) { root.setSpeed(root.snapSpeed(v)) }
          }

          Text {
            id: speedLabel
            text: root.snapSpeed(speedSlider.liveValue).toFixed(1) + "x"
            color: Color.popups.text
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            width: Style.space(48)
          }
        }

        Text {
          visible: root.speedError !== ""
          width: parent.width
          wrapMode: Text.WordWrap
          text: root.speedError
          color: Color.urgent
          font.family: Style.font.family
          font.pixelSize: Style.font.bodySmall
        }

        Text {
          visible: root.daemonUnavailable
          width: parent.width
          wrapMode: Text.WordWrap
          text: "Daemon isn't running — changes will apply once it starts."
          color: Qt.darker(Color.popups.text, 1.3)
          font.family: Style.font.family
          font.pixelSize: Style.font.bodySmall
          font.italic: true
        }

        PanelSeparator {}

        Text {
          objectName: "omatalkVersion"
          text: root.version
          color: Qt.darker(Color.popups.text, 1.3)
          font.family: Style.font.family
          font.pixelSize: Style.font.bodySmall
        }
      }
    }
  }
}
