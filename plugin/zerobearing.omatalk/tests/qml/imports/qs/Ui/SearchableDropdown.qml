import QtQuick

Item {
  property var options: []
  property string value: ""
  property string placeholderText: ""
  signal changed(string v)
}
