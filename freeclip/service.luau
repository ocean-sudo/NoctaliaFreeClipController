--!nonstrict
-- Owns the single freeclipd process. stdout state lines become noctalia.state
-- "snapshot". A settings change restarts this service, and the new daemon
-- takes the socket from the previous one.

local client = require("./lib/client.luau")

local RETRY_MS = 5000
local threshold = math.floor(tonumber(noctalia.getConfig("low_battery_percent")) or 15)
local notifyLow = noctalia.getConfig("low_battery_notify") ~= false

local streaming = false
local dipped = false

local function blank(phase, err)
  return {
    type = "state",
    seq = 0,
    phase = phase,
    paired = false,
    audio_connected = false,
    spp_connected = false,
    error = err,
    info = {},
    battery = {},
    gestures = {},
    gesture_options = {},
    dual_connect = { devices = {} },
    equalizer_options = {},
    language_options = {},
  }
end

local function publish(snap)
  noctalia.state.set("snapshot", snap)
end

local function earLevel(battery)
  if type(battery) ~= "table" then
    return nil
  end
  local best = nil
  for _, key in ipairs({ "left", "right" }) do
    local value = battery[key]
    if type(value) == "number" and (best == nil or value < best) then
      best = value
    end
  end
  return best
end

local function noteBattery(snap)
  if not notifyLow or snap.spp_connected ~= true then
    return
  end
  local level = earLevel(snap.battery)
  if level == nil then
    return
  end
  if level > threshold + 5 then
    dipped = false
  end
  if level <= threshold and not dipped then
    dipped = true
    local percent = math.floor(level)
    noctalia.notify(
      noctalia.tr("notify.low_battery_title"),
      noctalia.tr("notify.low_battery_body", { percent = percent })
    )
    noctalia.osd.show("battery-exclamation", tostring(percent) .. "%")
  end
end

local function onLine(line)
  if string.sub(line, 1, 1) ~= "{" then
    return
  end
  local snap = noctalia.json.decode(line)
  if type(snap) ~= "table" or snap.type ~= "state" then
    return
  end
  noteBattery(snap)
  publish(snap)
end

local function start()
  if streaming then
    noctalia.setUpdateInterval(60000)
    return
  end
  local command = client.serveCommand()
  if command == nil then
    publish(blank("unavailable", "binary"))
    noctalia.setUpdateInterval(RETRY_MS)
    return
  end
  local wrapped = "while true; do " .. command .. "; sleep 1; done"
  streaming = noctalia.runStream(wrapped, onLine) == true
  if streaming then
    noctalia.setUpdateInterval(60000)
  else
    publish(blank("unavailable", "stream"))
    noctalia.setUpdateInterval(RETRY_MS)
  end
end

publish(blank("starting", nil))

function update()
  start()
end

function onIpc(event, payload)
  if event == "refresh" then
    client.send({ cmd = "refresh" })
    return
  end
  if event ~= "latency" then
    return
  end
  local snap = noctalia.state.get("snapshot") or {}
  local value = nil
  if payload == "on" then
    value = true
  elseif payload == "off" then
    value = false
  elseif payload == "toggle" and type(snap.low_latency) == "boolean" then
    value = not snap.low_latency
  end
  if value ~= nil then
    client.send({ cmd = "set", key = "low_latency", value = value })
  end
end
