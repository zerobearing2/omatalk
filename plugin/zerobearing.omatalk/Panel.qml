import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// Voice + speed config panel once the Daemon launcher exists. Until then
// this is a setup screen that links to the site's install instructions; the
// plugin itself never downloads or runs an installer. Config CLI processes
// stay stopped while ~/.local/bin/omatalk is missing.
Panel {
  id: root
  moduleName: "zerobearing.omatalk"

  property var anchorItem: null
  property bool daemonUnavailable: false
  property bool daemonInstalled: false
  readonly property string installUrl: "https://omatalk.zerobearing.com/#install"

  readonly property var englishPrefixes: ["af_", "am_", "bf_", "bm_"]
  readonly property bool showingSetup: !daemonInstalled
  property string launcherPath: {
    var home = root.envText("HOME")
    if (home !== "") return home + "/.local/bin/omatalk"
    return ""
  }

  // Quickshell.env returns null when unset; String(null) is "null", which
  // would make launcherPath unusable.
  function envText(name) {
    var v = Quickshell.env(name)
    if (v === null || v === undefined) return ""
    return String(v)
  }

  // PanelSlider only snaps `step` for wheel nudges — dragging reports
  // continuous precision, snapping is the caller's job per its own docs —
  // so both the live label and the committed value round through this.
  function snapSpeed(v) { return Math.round(v * 10) / 10 }

  property var voiceOptions: []
  property string voice: ""
  property real speed: 1.0
  property string voiceError: ""
  property string speedError: ""
  property string daemonVersion: "unknown"
  property string pluginVersion: "unknown"
  readonly property string versionsLabel: {
    var line = "plugin " + pluginVersion
    if (root.daemonInstalled) line += " · omatalk " + daemonVersion
    return line
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

  // manifest.json lives in the plugin root (store clone or this checkout).
  function pluginFile(name) {
    var url = String(Qt.resolvedUrl(name))
    if (url.indexOf("file://") === 0) return url.slice(7)
    return url
  }

  function openInstallPage() {
    openProc.command = ["xdg-open", root.installUrl]
    openProc.running = true
  }

  function refresh() {
    if (!root.daemonInstalled || root.launcherPath === "") return
    var bin = root.launcherPath
    voicesProc.command = [bin, "config", "voices", "--json"]
    getProc.command = [bin, "config", "get", "--json"]
    versionProc.command = [bin, "version"]
    voicesProc.running = true
    getProc.running = true
    versionProc.running = true
  }

  onOpenedChanged: if (opened && root.daemonInstalled) refresh()
  onDaemonInstalledChanged: if (opened && root.daemonInstalled) refresh()

  function setVoice(value) {
    root.voice = value
    setVoiceProc.command = [root.launcherPath, "config", "set", "voice", value]
    setVoiceProc.running = true
    // Not sequenced after setVoiceProc: the preview never touches
    // config.toml or waits on the Daemon's reload, so there is nothing to
    // wait for — it fires in parallel with the save.
    previewProc.command = [root.launcherPath, "speak", "--voice", value, root.sampleTextFor(value)]
    previewProc.running = true
  }

  function setSpeed(value) {
    root.speed = value
    setSpeedProc.command = [root.launcherPath, "config", "set", "speed", String(value)]
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

  Process { id: openProc }

  // Plugin version is this checkout's manifest.json, not `omatalk version`.
  FileView {
    id: manifestFile
    path: root.pluginFile("manifest.json")
    printErrors: false
    onLoaded: {
      try {
        var parsed = JSON.parse(text())
        if (parsed.version) root.pluginVersion = String(parsed.version)
      } catch (e) {
        root.pluginVersion = "unknown"
      }
    }
    onLoadFailed: root.pluginVersion = "unknown"
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

        Text {
          text: "Omatalk"
          color: Color.popups.text
          font.family: Style.font.family
          font.pixelSize: Style.font.display
          font.bold: true
        }

        Column {
          visible: !root.daemonInstalled
          width: parent.width
          spacing: Style.space(14)

          Text {
            objectName: "omatalkSetupNote"
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Omatalk's speech Daemon is not installed.\n\nInstall instructions:\n" + root.installUrl
            color: Color.popups.text
            font.family: Style.font.family
            font.pixelSize: Style.font.body
          }

          Button {
            objectName: "omatalkOpenInstallButton"
            width: parent.width
            text: "Open install instructions"
            bordered: true
            foreground: Color.popups.text
            fontFamily: Style.font.family
            onClicked: {
              root.openInstallPage()
              root.close()
            }
          }
        }

        Column {
          visible: root.daemonInstalled
          width: parent.width
          spacing: Style.space(14)

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
        }

        Text {
          objectName: "omatalkVersions"
          text: root.versionsLabel
          color: Qt.darker(Color.popups.text, 1.6)
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }
      }
    }
  }
}
