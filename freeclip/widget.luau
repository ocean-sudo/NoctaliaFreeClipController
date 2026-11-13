--!nonstrict
-- Bar battery. Click opens the panel near this widget. Scroll changes the
-- system volume through Noctalia. Right click asks the daemon to refresh.

local client = require("./lib/client.luau")
local labels = require("./lib/labels.luau")

local PANEL_ID = "zigzag/freeclip:panel"

local function percent(value)
  if type(value) ~= "number" then
    return nil
  end
  return math.floor(value)
end

local function collect(battery, style, showCase)
  battery = type(battery) == "table" and battery or {}
  local values = {}
  local left = percent(battery.left)
  local right = percent(battery.right)
  local caseLevel = percent(battery.case)
  if style == "left" then
    if left ~= nil then
      table.insert(values, left)
    end
  elseif style == "right" then
    if right ~= nil then
      table.insert(values, right)
    end
  elseif style == "both" then
    if left ~= nil then
      table.insert(values, left)
    end
    if right ~= nil then
      table.insert(values, right)
    end
    if showCase and caseLevel ~= nil then
      table.insert(values, caseLevel)
    end
  else
    if left ~= nil then
      table.insert(values, left)
    end
    if right ~= nil then
      table.insert(values, right)
    end
    if showCase and caseLevel ~= nil then
      table.insert(values, caseLevel)
    end
    if #values == 0 then
      local global = percent(battery.global)
      if global ~= nil then
        table.insert(values, global)
      end
    end
  end
  return values, left, right, caseLevel
end

local function batteryText(battery, style, showCase)
  local values, left, right, caseLevel = collect(battery, style, showCase)
  if style == "both" then
    local parts = {}
    if left ~= nil then
      table.insert(parts, "L" .. tostring(left))
    end
    if right ~= nil then
      table.insert(parts, "R" .. tostring(right))
    end
    if showCase and caseLevel ~= nil then
      table.insert(parts, "C" .. tostring(caseLevel))
    end
    return table.concat(parts, " "), values
  end
  if #values == 0 then
    return "", values
  end
  local lowest = values[1]
  for index = 2, #values do
    if values[index] < lowest then
      lowest = values[index]
    end
  end
  return tostring(lowest), values
end

local function shownPhase(phase)
  return phase == "starting" or phase == "connecting" or phase == "ready" or phase == "reconnecting"
end

local function tooltip(snap, left, right, caseLevel)
  local rows = {
    { noctalia.tr("widget.tooltip.phase"), labels.phase(snap.phase) },
  }
  local function add(label, value)
    if value ~= nil then
      table.insert(rows, { label, tostring(value) .. "%" })
    end
  end
  add(noctalia.tr("widget.tooltip.left"), left)
  add(noctalia.tr("widget.tooltip.right"), right)
  add(noctalia.tr("widget.tooltip.case"), caseLevel)
  if snap.wearing == true then
    table.insert(rows, { noctalia.tr("widget.tooltip.wearing"), noctalia.tr("widget.wearing.on") })
  elseif snap.wearing == false then
    table.insert(rows, { noctalia.tr("widget.tooltip.wearing"), noctalia.tr("widget.wearing.off") })
  else
    table.insert(rows, { noctalia.tr("widget.tooltip.wearing"), noctalia.tr("widget.tooltip.unknown") })
  end
  local err = labels.error(snap.error)
  if err ~= nil then
    table.insert(rows, { noctalia.tr("widget.tooltip.error"), err })
  end
  return rows
end

local function render(snap)
  local style = noctalia.getConfig("battery_style") or "min"
  local showCase = noctalia.getConfig("show_case") == true
  local hide = noctalia.getConfig("hide_when_disconnected") ~= false
  local threshold = math.floor(tonumber(noctalia.getConfig("low_battery_percent")) or 15)
  if type(snap) ~= "table" then
    snap = {}
  end
  local phase = snap.phase or "starting"
  if hide and not shownPhase(phase) then
    barWidget.setVisible(false)
    return
  end
  local text, values, left, right, caseLevel = batteryText(snap.battery, style, showCase)
  local _, leftLevel, rightLevel, caseLevelShown = collect(snap.battery, style, showCase)
  local low = false
  for _, value in ipairs(values) do
    if value <= threshold then
      low = true
    end
  end
  local connected = snap.spp_connected == true or phase == "ready" or phase == "connecting"
  barWidget.setVisible(true)
  barWidget.setGlyph(connected and "headphones" or "headphones-off")
  barWidget.setText(text)
  barWidget.setColor(low and "error" or "on_surface")
  barWidget.setGlyphColor(low and "error" or (snap.spp_connected == true and "primary" or "on_surface"))
  barWidget.setTooltip(tooltip(snap, leftLevel, rightLevel, caseLevelShown))
end

noctalia.state.watch("snapshot", render)
render(noctalia.state.get("snapshot"))

function onClick()
  noctalia.togglePanel(PANEL_ID)
end

function onRightClick()
  client.send({ cmd = "refresh" }, function(ack)
    if ack.ok == false then
      noctalia.notifyError(noctalia.tr("notify.command_failed"), tostring(ack.error or ""))
    end
  end)
end

function onScroll(axis, steps)
  if axis ~= "vertical" or type(steps) ~= "number" or steps == 0 then
    return
  end
  local verb = steps > 0 and "volume-down" or "volume-up"
  local amount = tostring(math.max(1, math.floor(math.abs(steps) * 5)))
  noctalia.runAsync({ "noctalia", "msg", verb, amount })
end
