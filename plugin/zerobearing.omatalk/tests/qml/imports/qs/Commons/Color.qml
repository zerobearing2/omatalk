pragma Singleton
import QtQuick

QtObject {
  property color foreground: "#dddddd"
  property color urgent: "#ff7b72"
  property color accent: "#79c0ff"
  readonly property QtObject popups: QtObject {
    property color text: "#dddddd"
    property color background: "#1a1a1a"
    property color border: "#79c0ff"
  }
}
