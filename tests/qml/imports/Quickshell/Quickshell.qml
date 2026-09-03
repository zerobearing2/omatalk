pragma Singleton
import QtQml

QtObject {
  property var detachedCommands: []
  property var envValues: ({ "XDG_RUNTIME_DIR": "/tmp" })

  function execDetached(command) {
    var commands = detachedCommands.slice()
    commands.push(command)
    detachedCommands = commands
  }

  function env(name) {
    if (envValues[name] !== undefined) return envValues[name]
    return ""
  }

  function setEnv(name, value) {
    var next = {}
    for (var key in envValues) next[key] = envValues[key]
    next[name] = value
    envValues = next
  }
}
