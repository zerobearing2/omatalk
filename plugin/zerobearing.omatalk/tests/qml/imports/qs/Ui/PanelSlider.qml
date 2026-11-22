import QtQuick

Item {
  property var bar
  property real minimum: 0
  property real maximum: 1
  property real step: 0.1
  property real value: 0
  property real liveValue: value
  property int tickCount: 0
  property color tickColor
  signal released(real v)
  signal moved(real v)
}
