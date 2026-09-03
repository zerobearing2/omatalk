import QtQml

QtObject {
  id: root

  property bool running: false
  property var command: []
  property var stdout: null
  property var stderr: null
  property bool _completing: false

  signal exited(int exitCode, int exitStatus)

  onRunningChanged: if (!running && !_completing) exited(143, 1)

  function complete(exitCode, stdoutText, stderrText) {
    if (stdout) {
      stdout.text = String(stdoutText || "")
      stdout.streamFinished()
    }
    if (stderr) {
      stderr.text = String(stderrText || "")
      stderr.streamFinished()
    }
    _completing = true
    running = false
    _completing = false
    exited(exitCode, 0)
  }

  Component.onCompleted: ProcessRegistry.add(root)
  Component.onDestruction: ProcessRegistry.remove(root)
}
