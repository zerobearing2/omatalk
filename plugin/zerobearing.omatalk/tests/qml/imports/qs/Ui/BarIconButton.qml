import QtQuick

Item {
  property var bar: null
  property bool active: false
  property color activeColor
  property bool useActiveColor: false
  property string tooltipText: ""
  property string text: ""
  signal pressed(int b)
}
