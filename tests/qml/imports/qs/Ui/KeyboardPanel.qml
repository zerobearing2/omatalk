import QtQuick

Item {
  property var anchorItem
  property var owner
  property var bar
  property bool open: false
  property var focusTarget
  property int contentWidth
  property int contentHeight

  function fittedContentWidth(w) { return w }
  function fittedContentHeight(h) { return h }
}
