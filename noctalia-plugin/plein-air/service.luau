--!nonstrict
-- Runs plein-air-wallpaper on an interval and on demand, and publishes the
-- current painting as "current" / "status" state for the widget and shortcut.

local CHECK_MS = 60 * 1000
local STALE_RUN_SECONDS = 5 * 60

local running = false
local runStartedAt = 0
local lastRunAt = 0

local function dir()
  return noctalia.expandPath(noctalia.getConfig("directory") or "~/Downloads/walls/plein-air")
end

local function metaPath()
  return `{dir()}/current.json`
end

local function publishCurrent()
  local raw = noctalia.readFile(metaPath())
  if not raw then
    return
  end
  local meta = noctalia.json.decode(raw)
  if type(meta) == "table" then
    noctalia.state.set("current", meta)
  end
end

local function setStatus(state, message)
  noctalia.state.set("status", { state = state, message = message or "" })
end

local function command()
  local args = { "plein-air-wallpaper", "--width", tostring(noctalia.getConfig("width") or 3840) }
  local mode = noctalia.getConfig("location_mode")
  local city = noctalia.string.trim(noctalia.getConfig("city") or "")
  local coords = noctalia.string.trim(noctalia.getConfig("coords") or "")
  if mode == "city" and city ~= "" then
    table.insert(args, "--city")
    table.insert(args, city)
  elseif mode == "coords" and coords ~= "" then
    table.insert(args, "--coords")
    table.insert(args, coords)
  end
  return args
end

-- "none" when disabled in settings or when the lutgen binary is missing.
local function palette()
  if noctalia.getConfig("use_lutgen") == false or not noctalia.commandExists("lutgen") then
    return "none"
  end
  return noctalia.getConfig("palette") or "catppuccin-macchiato"
end

local function publishRecolor()
  local wanted = noctalia.getConfig("use_lutgen") ~= false
  local available = noctalia.commandExists("lutgen")
  noctalia.state.set("recolor", {
    palette = palette(),
    missing = wanted and not available,
  })
end

local function env()
  return {
    PLEIN_AIR_DIR = dir(),
    PLEIN_AIR_CONTACT = noctalia.getConfig("contact") or "",
    PLEIN_AIR_PALETTE = palette(),
    PLEIN_AIR_LUTGEN_ARGS = noctalia.getConfig("lutgen_args") or "",
  }
end

local function shellQuote(s)
  return "'" .. s:gsub("'", "'\\''") .. "'"
end

-- runAsync's captured mode caps at 60s, and a fetch plus recolor can take longer,
-- so the run is detached and writes its own result files that update() polls.
local function refresh()
  if running then
    return
  end
  local d = dir()
  local ok, err = noctalia.mkdirAll(d)
  if not ok then
    setStatus("error", `Could not create {d}: {err}`)
    return
  end

  local parts = {}
  for k, v in pairs(env()) do
    table.insert(parts, `{k}={shellQuote(v)}`)
  end
  for _, a in ipairs(command()) do
    table.insert(parts, shellQuote(a))
  end
  local status = shellQuote(`{d}/.run-status`)
  local log = shellQuote(`{d}/.run-log`)
  local cmd = `rm -f {status}; env {table.concat(parts, " ")} >{log} 2>&1; echo $? >{status}`

  if not noctalia.runAsync({ "sh", "-c", cmd }) then
    setStatus("error", "Could not start plein-air-wallpaper")
    return
  end
  running = true
  runStartedAt = os.time()
  lastRunAt = runStartedAt
  setStatus("running")
end

local function checkRun()
  if not running then
    return
  end
  local d = dir()
  local raw = noctalia.readFile(`{d}/.run-status`)
  if not raw then
    if os.time() - runStartedAt > STALE_RUN_SECONDS then
      running = false
      setStatus("error", "Timed out")
    end
    return
  end
  running = false
  local code = tonumber(noctalia.string.trim(raw))
  if code == 0 then
    publishCurrent()
    setStatus("idle")
    return
  end
  local log = noctalia.readFile(`{d}/.run-log`) or ""
  local lastLine = log:match("([^\n]+)\n*$") or `exit code {code}`
  setStatus("error", lastLine)
  noctalia.notifyError(noctalia.tr("notify.failed"), lastLine)
end

local function due()
  local minutes = tonumber(noctalia.getConfig("interval_minutes")) or 0
  return minutes > 0 and os.time() - lastRunAt >= minutes * 60
end

noctalia.setUpdateInterval(CHECK_MS)

function update()
  checkRun()
  if due() then
    refresh()
  end
end

function onIpc(event, _payload)
  if event == "refresh" then
    refresh()
  end
end

noctalia.state.watch("refresh_request", function(value)
  if value ~= nil then
    refresh()
  end
end)

function onConfigChanged()
  publishCurrent()
  publishRecolor()
end

-- Poll faster while a run is in flight so the widget updates promptly.
noctalia.state.watch("status", function(value)
  noctalia.setUpdateInterval((type(value) == "table" and value.state == "running") and 2000 or CHECK_MS)
end)

publishCurrent()
publishRecolor()
setStatus("idle")
-- Treat the last saved painting as the last run so a restart doesn't refetch immediately.
local info = noctalia.fileInfo(metaPath())
lastRunAt = (type(info) == "table" and info.mtime) or 0
