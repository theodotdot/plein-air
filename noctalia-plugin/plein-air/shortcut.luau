--!nonstrict
-- Control-center tile: fetch a new painting.

local function render(status)
  local isRunning = type(status) == "table" and status.state == "running"
  shortcut.setLabel(noctalia.tr("shortcut.label"))
  shortcut.setIcon("palette")
  shortcut.setActive(isRunning)
end

noctalia.state.watch("status", render)

function onClick()
  noctalia.state.set("refresh_request", noctalia.nowMs())
end

function onRightClick()
  noctalia.openSettings()
end

render(noctalia.state.get("status"))
