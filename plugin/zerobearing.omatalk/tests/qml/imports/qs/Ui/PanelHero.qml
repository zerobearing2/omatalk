import QtQuick

Item {
  id: root

  property Component iconComponent: null
  property string title: ""
  property string meta: ""
  property string detail: ""
  property color foreground: "#dddddd"
  property string fontFamily: ""
  property real iconSize: 24
  property real iconOpacity: 1.0
  property Component trailingControl: null

  readonly property Item iconItem: iconLoader.item

  Loader {
    id: iconLoader
    sourceComponent: root.iconComponent
  }
}
