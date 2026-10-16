--!nonstrict
-- Panel opened from the bar widget: current painting, refresh, settings.

local function tr(key, args) return noctalia.tr(key, args) end

local WRAP = 340

local function recolorLine()
  local recolor = noctalia.state.get("recolor")
  if type(recolor) ~= "table" then
    return nil
  end
  if recolor.missing then
    return tr("panel.recolor_missing")
  end
  if recolor.palette == "none" then
    return tr("panel.recolor_off")
  end
  return tr("panel.recolor_on", { palette = recolor.palette })
end

local function render()
  local current = noctalia.state.get("current")
  local status = noctalia.state.get("status")
  local isRunning = type(status) == "table" and status.state == "running"
  local isError = type(status) == "table" and status.state == "error"

  -- The text block scrolls if it outgrows the panel, so the buttons below never get pushed out.
  local rows = {}

  if type(current) == "table" then
    local ctx = current.context or {}
    rows[#rows + 1] = ui.label({ text = current.title or "", fontSize = 15, fontWeight = "medium", maxWidth = WRAP, maxLines = 2 })
    rows[#rows + 1] = ui.label({
      text = `{current.artist or ""}{(current.year and current.year ~= "") and ", " .. tostring(current.year) or ""}`,
      maxWidth = WRAP, opacity = 0.75,
    })
    rows[#rows + 1] = ui.label({
      text = `{ctx.weather or "?"} · {ctx.season or "?"} · {ctx.time or "?"}{ctx.place and ctx.place ~= "" and " · " .. ctx.place or ""}`,
      maxWidth = WRAP, fontSize = 12, opacity = 0.6,
    })
    rows[#rows + 1] = ui.label({ text = current.source or "", fontSize = 12, opacity = 0.6 })
  else
    rows[#rows + 1] = ui.label({ text = tr("panel.nothing"), opacity = 0.7 })
  end

  local recolor = recolorLine()
  if recolor then
    rows[#rows + 1] = ui.label({ text = recolor, fontSize = 12, opacity = 0.6 })
  end
  if isError then
    rows[#rows + 1] = ui.label({ text = status.message or "", color = "error", fontSize = 12, maxWidth = WRAP, maxLines = 3 })
  end

  local buttons = ui.row({ gap = 8, align = "center" }, {
    ui.button({
      text = isRunning and tr("widget.refreshing") or tr("panel.refresh"),
      glyph = isRunning and "loader-2" or "refresh",
      variant = "primary",
      controlSize = "md",
      contentAlign = "center",
      enabled = not isRunning,
      flexGrow = 1,
      minWidth = 0,
      onClick = function() noctalia.state.set("refresh_request", noctalia.nowMs()) end,
    }),
    ui.button({
      text = tr("panel.settings"),
      glyph = "settings",
      variant = "secondary",
      controlSize = "md",
      contentAlign = "center",
      flexGrow = 1,
      minWidth = 0,
      onClick = function()
        panel.close()
        noctalia.openSettings()
      end,
    }),
  })

  panel.render(ui.column({ paddingH = 14, paddingV = 14, gap = 10, flexGrow = 1 }, {
    ui.row({ align = "center", gap = 8 }, {
      ui.label({ text = tr("panel.title"), fontSize = 16, fontWeight = "bold", flexGrow = 1 }),
      ui.button({ glyph = "close", variant = "ghost", onClick = function() panel.close() end }),
    }),
    ui.separator({}),
    ui.scroll({ flexGrow = 1, gap = 6 }, rows),
    ui.separator({}),
    buttons,
  }))
end

function onOpen(_context)
  render()
end

noctalia.state.watch("current", render)
noctalia.state.watch("status", render)
noctalia.state.watch("recolor", render)
