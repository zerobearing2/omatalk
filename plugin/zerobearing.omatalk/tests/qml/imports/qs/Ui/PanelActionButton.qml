import QtQuick

Item {
  property string iconText: ""
  property string tooltipText: ""
  property color foreground: "#dddddd"
  property color hoverColor: foreground
  property string fontFamily: ""
  property real fontSize: 14
  property real size: 22
  property bool focusable: false
  property bool hasCursor: false
  property bool bordered: false

  implicitWidth: size
  implicitHeight: size
  width: implicitWidth
  height: implicitHeight

  signal clicked()
  signal hovered(bool isHovered)
}
