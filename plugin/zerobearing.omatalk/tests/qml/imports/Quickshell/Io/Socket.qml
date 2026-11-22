import QtQml

QtObject {
  property string path: ""
  property bool connected: false
  property var parser: null

  signal connectionStateChanged()
  signal error(var err)

  function write(data) {}
  function flush() {}
}
