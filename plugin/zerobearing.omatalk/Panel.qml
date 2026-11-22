import QtQuick
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
  id: root
  moduleName: "zerobearing.omatalk"

  property var anchorItem: null
  property bool daemonUnavailable: false
  property string daemonState: "idle"
  property string glyph: ""

  readonly property var englishPrefixes: ["af_", "am_", "bf_", "bm_"]
  readonly property string launcherPath: "/usr/bin/omatalk"

  // PanelSlider only snaps `step` for wheel nudges — dragging reports
  // continuous precision, snapping is the caller's job per its own docs —
  // so both the live label and the committed value round through this.
  function snapSpeed(v) { return Math.round(v * 10) / 10 }

  readonly property int wakeLeadMax: 2000
  readonly property int wakeLeadStep: 50
  function snapWakeLead(v) {
    var snapped = Math.round(v / wakeLeadStep) * wakeLeadStep
    return Math.max(0, Math.min(wakeLeadMax, snapped))
  }
  function wakeLeadLabel(ms) { return ms === 0 ? "Off" : ms + " ms" }

  property var voiceOptions: []
  property string voice: ""
  property real speed: 1.0
  property bool pauseMedia: true
  property int wakeLeadMs: 650
  property string daemonVersion: "unknown"

  readonly property string statusLine: {
    if (daemonUnavailable) return "Daemon not running"
    if (daemonState === "speaking") return "Speaking…"
    return "Local TTS · F8 to speak"
  }

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
    var bin = root.launcherPath
    voicesProc.command = [bin, "config", "voices", "--json"]
    getProc.command = [bin, "config", "get", "--json"]
    versionProc.command = [bin, "version"]
    voicesProc.running = true
    getProc.running = true
    versionProc.running = true
  }

  onOpenedChanged: if (opened) refresh()

  // Not sequenced after the voice save: the preview never touches
  // config.toml or waits on the Daemon's reload, so there is nothing to
  // wait for — it fires in parallel with the save.
  function previewVoice(name) {
    previewProc.command = [root.launcherPath, "speak", "--voice", name, root.sampleTextFor(name)]
    previewProc.running = true
  }

  // `config set <key>`. Its stderr line is the error shown under the
  // control until a later save of the same key succeeds.
  component ConfigSetter: Process {
    id: setter
    property string key
    property string error: ""
    function save(value) {
      setter.command = [root.launcherPath, "config", "set", setter.key, String(value)]
      setter.running = true
    }
    stdout: StdioCollector { waitForEnd: true }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: setter.error = text.trim()
    }
    onExited: function(exitCode) { if (exitCode === 0) setter.error = "" }
  }

  component ErrorLine: Text {
    visible: text !== ""
    width: parent.width
    wrapMode: Text.WordWrap
    color: Color.urgent
    font.family: Style.font.family
    font.pixelSize: Style.font.bodySmall
  }

  ConfigSetter { id: voiceSetter; key: "voice" }
  ConfigSetter { id: speedSetter; key: "speed" }
  ConfigSetter { id: pauseMediaSetter; key: "pause_media" }
  ConfigSetter { id: wakeLeadSetter; key: "wake_lead_ms" }

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
          if (cfg.pause_media !== undefined) root.pauseMedia = cfg.pause_media
          if (cfg.wake_lead_ms !== undefined) root.wakeLeadMs = cfg.wake_lead_ms
        } catch (e) {
          // Leave the previous values in place on a bad/empty response.
        }
      }
    }
  }

  Process {
    id: previewProc
    stdout: StdioCollector { waitForEnd: true }
    stderr: StdioCollector { waitForEnd: true }
  }

  Process {
    id: versionProc
    command: ["omatalk", "version"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var next = text.trim()
        root.daemonVersion = next !== "" ? next : "unknown"
      }
    }
    // Unlike voices/get, which keep last-known functional state on a bad
    // reply, version is a label. A failed or empty lookup must not keep
    // showing a stale release number — "unknown" is the honest fallback.
    onExited: function(exitCode) { if (exitCode !== 0) root.daemonVersion = "unknown" }
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

        PanelHero {
          objectName: "omatalkHero"
          width: parent.width
          title: "Omatalk"
          meta: root.statusLine
          detail: root.daemonVersion
          foreground: Color.popups.text
          fontFamily: Style.font.family

          iconComponent: Component {
            Text {
              text: root.glyph
              color: Color.popups.text
              font.family: Style.font.family
              font.pixelSize: Style.font.display
            }
          }
        }

        PanelSeparator {}

        Column {
          width: parent.width
          spacing: Style.space(14)

          PanelSectionHeader { text: "VOICE" }

          Row {
            width: parent.width
            spacing: Style.space(8)

            SearchableDropdown {
              id: voiceDropdown
              objectName: "omatalkVoiceDropdown"
              width: parent.width - previewButton.width - parent.spacing
              options: root.voiceOptions
              placeholderText: "Search voices…"
              onChanged: function(v) {
                root.voice = v
                voiceSetter.save(v)
                root.previewVoice(v)
              }

              Binding on value { value: root.voice }
            }

            PanelActionButton {
              id: previewButton
              objectName: "omatalkPreviewButton"
              anchors.verticalCenter: parent.verticalCenter
              iconText: "󰐊"
              tooltipText: "Preview voice"
              foreground: Color.popups.text
              fontFamily: Style.font.family
              bordered: true
              enabled: root.voice !== ""
              onClicked: root.previewVoice(root.voice)
            }
          }

          ErrorLine { objectName: "omatalkVoiceError"; text: voiceSetter.error }

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
              onReleased: function(v) {
                root.speed = root.snapSpeed(v)
                speedSetter.save(root.speed)
              }
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

          ErrorLine { objectName: "omatalkSpeedError"; text: speedSetter.error }

          PanelSeparator {}

          PanelSectionHeader { text: "PLAYBACK" }

          Toggle {
            objectName: "omatalkPauseMediaToggle"
            width: parent.width
            label: "Pause media while speaking"
            description: "Spotify, browsers and video pause, then resume when omatalk finishes."
            foreground: Color.popups.text
            fontFamily: Style.font.family
            checked: root.pauseMedia
            onClicked: {
              root.pauseMedia = !root.pauseMedia
              pauseMediaSetter.save(root.pauseMedia)
            }
          }

          ErrorLine { objectName: "omatalkPauseMediaError"; text: pauseMediaSetter.error }

          Column {
            width: parent.width
            spacing: Style.space(2)

            Text {
              width: parent.width
              text: "Bluetooth and HDMI wake"
              color: Color.popups.text
              font.family: Style.font.family
              font.pixelSize: Style.font.subtitle
              font.bold: true
            }

            Text {
              objectName: "omatalkWakeLeadHelp"
              width: parent.width
              wrapMode: Text.WordWrap
              text: "Silence before speech so headphones don't clip the first word."
              color: Qt.darker(Color.popups.text, 1.5)
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
          }

          Row {
            width: parent.width
            spacing: Style.space(12)

            PanelSlider {
              id: wakeLeadSlider
              objectName: "omatalkWakeLeadSlider"
              bar: root.bar
              width: parent.width - wakeLeadLabelText.width - Style.space(12)
              minimum: 0
              maximum: root.wakeLeadMax
              step: root.wakeLeadStep
              value: root.wakeLeadMs
              tickCount: 5
              tickColor: Color.popups.background
              onReleased: function(v) {
                root.wakeLeadMs = root.snapWakeLead(v)
                wakeLeadSetter.save(root.wakeLeadMs)
              }
            }

            Text {
              id: wakeLeadLabelText
              objectName: "omatalkWakeLeadLabel"
              text: root.wakeLeadLabel(root.snapWakeLead(wakeLeadSlider.liveValue))
              color: Color.popups.text
              font.family: Style.font.family
              font.pixelSize: Style.font.body
              width: Style.space(64)
            }
          }

          ErrorLine { objectName: "omatalkWakeLeadError"; text: wakeLeadSetter.error }

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
        }
      }
    }
  }
}
