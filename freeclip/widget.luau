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

local function budList(battery)
  local buds = {}
  if type(battery.buds) ~= "table" then
    return buds
  end
  for _, bud in ipairs(battery.buds) do
    if type(bud) == "table" then
      table.insert(buds, bud)
    end
  end
  return buds
end

-- Live charge only. A bud in the case keeps its last reading for the panel,
-- but that number is not what is on an ear right now.
local function liveLevel(bud)
  if type(bud) ~= "table" or bud.position == "case" then
    return nil
  end
  local level = percent(bud.level)
  if level == nil or level <= 0 or level > 100 then
    return nil
  end
  return level
end

local function levelAt(buds, place)
  for _, bud in ipairs(buds) do
    if bud.position == place then
      return liveLevel(bud)
    end
  end
  return nil
end

local function collect(battery, style, showCase)
  battery = type(battery) == "table" and battery or {}
  local buds = budList(battery)
  local caseLevel = percent(battery.case)
  local values = {}
  if style == "left" or style == "right" then
    local level = levelAt(buds, style)
    if level ~= nil then
      table.insert(values, level)
    end
  else
    for _, bud in ipairs(buds) do
      local level = liveLevel(bud)
      if level ~= nil then
        table.insert(values, level)
      end
    end
    if showCase and caseLevel ~= nil then
      table.insert(values, caseLevel)
    end
    if #values == 0 then
      local global = percent(battery.global)
      if global ~= nil and global > 0 and global <= 100 then
        table.insert(values, global)
      end
    end
  end
  return values, buds, caseLevel
end

local function batteryText(battery, style, showCase)
  local values, buds, caseLevel = collect(battery, style, showCase)
  if style == "both" then
    local parts = {}
    for _, place in ipairs({ "left", "right" }) do
      local level = levelAt(buds, place)
      if level ~= nil then
        local prefix = place == "left" and "L" or "R"
        table.insert(parts, prefix .. tostring(level))
      end
    end
    for _, bud in ipairs(buds) do
      if bud.position ~= "left" and bud.position ~= "right" then
        local level = liveLevel(bud)
        if level ~= nil then
          table.insert(parts, tostring(bud.id) .. ":" .. tostring(level))
        end
      end
    end
    if showCase and caseLevel ~= nil then
      table.insert(parts, "C" .. tostring(caseLevel))
    end
    return table.concat(parts, " "), values, buds, caseLevel
  end
  if #values == 0 then
    return "", values, buds, caseLevel
  end
  local lowest = values[1]
  for index = 2, #values do
    if values[index] < lowest then
      lowest = values[index]
    end
  end
  return tostring(lowest), values, buds, caseLevel
end

local function shownPhase(phase)
  return phase == "starting" or phase == "connecting" or phase == "ready" or phase == "reconnecting"
end

local function placeLabel(position)
  if position == "left" then
    return noctalia.tr("panel.left")
  end
  if position == "right" then
    return noctalia.tr("panel.right")
  end
  if position == "case" then
    return noctalia.tr("panel.in_case")
  end
  return noctalia.tr("panel.unreported")
end

local function tooltip(snap, buds, caseLevel)
  local rows = {
    { noctalia.tr("widget.tooltip.phase"), labels.phase(snap.phase) },
  }
  for _, bud in ipairs(buds) do
    local text = placeLabel(bud.position)
    local level = percent(bud.level)
    if level ~= nil and level > 0 and level <= 100 then
      text = text .. " " .. tostring(level) .. "%"
    end
    table.insert(rows, { noctalia.tr("panel.bud", { n = bud.id }), text })
  end
  if caseLevel ~= nil then
    table.insert(rows, { noctalia.tr("widget.tooltip.case"), tostring(caseLevel) .. "%" })
  end
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
  local text, values, buds, caseLevel = batteryText(snap.battery, style, showCase)
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
  barWidget.setTooltip(tooltip(snap, buds, caseLevel))
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
