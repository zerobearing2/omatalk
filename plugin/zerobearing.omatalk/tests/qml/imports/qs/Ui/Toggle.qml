import QtQuick

Item {
  property string label: ""
  property string description: ""
  property bool checked: false
  property color foreground: "#dddddd"
  property string fontFamily: ""
  signal clicked()
}
