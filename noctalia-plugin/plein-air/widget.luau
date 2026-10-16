--!nonstrict
-- Bar widget: shows the current painting, click for a new one.

local function tr(key, args) return noctalia.tr(key, args) end

local showText = noctalia.getConfig("show_text")
local maxChars = tonumber(noctalia.getConfig("max_chars")) or 32

local current = noctalia.state.get("current")
local status = noctalia.state.get("status")

local function truncate(text)
  if #text <= maxChars then
    return text
  end
  return text:sub(1, maxChars - 1) .. "…"
end

local function render()
  local isRunning = type(status) == "table" and status.state == "running"
  local isError = type(status) == "table" and status.state == "error"
  barWidget.setGlyph(isRunning and "loader-2" or (isError and "photo-exclamation" or "palette"))
  barWidget.setGlyphColor(isError and "error" or "on_surface")

  if isRunning then
    barWidget.setText(showText and tr("widget.refreshing") or "")
    barWidget.setTooltip(tr("widget.refreshing"))
    return
  end

  if type(current) ~= "table" then
    barWidget.setText("")
    barWidget.setTooltip(isError and status.message or tr("widget.idle"))
    return
  end

  barWidget.setText(showText and truncate(current.title or "") or "")
  local ctx = current.context or {}
  local rows = {
    { current.title or "", current.artist or "" },
    { "Year", tostring(current.year or "") },
    { "Source", current.source or "" },
    { "Matched", `{ctx.weather or "?"} · {ctx.season or "?"} · {ctx.time or "?"} · {ctx.place or ""}` },
  }
  if isError then
    table.insert(rows, { "Last run failed", status.message or "" })
  end
  table.insert(rows, { tr("widget.hint"), "" })
  barWidget.setTooltip(rows)
end

noctalia.state.watch("current", function(value)
  current = value
  render()
end)
noctalia.state.watch("status", function(value)
  status = value
  render()
end)

function onClick()
  noctalia.togglePanel("theodotdot/plein-air:panel")
end

function onRightClick()
  noctalia.state.set("refresh_request", noctalia.nowMs())
end

render()
