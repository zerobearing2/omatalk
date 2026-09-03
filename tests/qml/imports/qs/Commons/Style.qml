pragma Singleton
import QtQuick

QtObject {
  function space(n) { return n }
  readonly property QtObject font: QtObject {
    property string family: "sans"
    property int display: 16
    property int body: 12
    property int bodySmall: 10
  }
}
