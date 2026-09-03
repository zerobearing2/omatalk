pragma Singleton
import QtQuick

QtObject {
  property color foreground: "#dddddd"
  property color urgent: "#ff7b72"
  readonly property QtObject popups: QtObject {
    property color text: "#dddddd"
    property color background: "#1a1a1a"
  }
}
