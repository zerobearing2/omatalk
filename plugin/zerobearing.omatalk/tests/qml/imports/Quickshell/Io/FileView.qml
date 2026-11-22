import QtQml

QtObject {
  id: root

  property url path
  property bool watchChanges: false
  property bool printErrors: false
  property string _text: '{"version":"1.0.0"}'

  signal loaded()
  signal loadFailed(var error)
  signal fileChanged()

  function text() { return _text }

  function reload() { loaded() }

  Component.onCompleted: loaded()
}
