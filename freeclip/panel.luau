--!nonstrict
-- FreeClip 2 panel. The stdout snapshot is the source of truth: a control
-- reports the user's change, and the next state line redraws it. A rejected
-- command notifies and redraws the previous snapshot.

local client = require("./lib/client.luau")
local labels = require("./lib/labels.luau")

local open = false
local closeButton = true
local volume = 0
local muted = false
local volumeKnown = false
local polling = false
local render

local EQ_FALLBACK = { "default", "hardbass", "treble", "voice" }
local QUALITY = { "connectivity", "quality" }
local GESTURES = {
  { id = "double_tap_left", options = "double_tap", label = "panel.double_left" },
  { id = "double_tap_right", options = "double_tap", label = "panel.double_right" },
  { id = "triple_tap_left", options = "triple_tap", label = "panel.triple_left" },
  { id = "triple_tap_right", options = "triple_tap", label = "panel.triple_right" },
  { id = "long_tap", options = "long_tap", label = "panel.long_tap" },
  { id = "swipe", options = "swipe", label = "panel.swipe" },
  { id = "noise_control", options = "noise_control", label = "panel.noise" },
}

local function snapshot()
  local snap = noctalia.state.get("snapshot")
  if type(snap) ~= "table" then
    return {}
  end
  return snap
end

local function send(request)
  client.send(request, function(ack)
    if type(ack) == "table" and ack.ok == false then
      noctalia.notifyError(noctalia.tr("notify.command_failed"), tostring(ack.error or ""))
      if open then
        render()
      end
    end
  end)
end

local function percent(value)
  if type(value) ~= "number" then
    return nil
  end
  return math.max(0, math.min(100, math.floor(value)))
end

local function strings(values)
  local list = {}
  if type(values) ~= "table" then
    return list
  end
  for _, item in ipairs(values) do
    if type(item) == "string" and item ~= "" then
      table.insert(list, item)
    end
  end
  return list
end

local function withCurrent(values, current)
  local list = strings(values)
  if type(current) == "string" and current ~= "" then
    local found = false
    for _, item in ipairs(list) do
      if item == current then
        found = true
      end
    end
    if not found then
      table.insert(list, current)
    end
  end
  return list
end

local function selectedIndex(options, current)
  if type(current) ~= "string" then
    return nil
  end
  for index, item in ipairs(options) do
    if item == current then
      return index - 1
    end
  end
  return nil
end

local function section(key, title, child)
  return ui.column({ key = key, gap = 6, align = "stretch" }, {
    ui.label({ text = title, fontSize = 12, fontWeight = "medium", color = "on_surface_variant" }),
    child,
  })
end

local function choiceSelect(key, ids, current, textFor, onPick)
  local shown = {}
  for _, id in ipairs(ids) do
    table.insert(shown, textFor(id))
  end
  return ui.select({
    key = key,
    options = shown,
    selectedIndex = selectedIndex(ids, current),
    placeholder = noctalia.tr("panel.unknown"),
    onChange = function(index)
      local choice = ids[(tonumber(index) or 0) + 1]
      if choice ~= nil and choice ~= current then
        onPick(choice)
      end
    end,
  })
end

local function toggleRow(key, label, checked, onChange)
  return ui.row({ key = key, gap = 12, align = "center" }, {
    ui.toggle({ key = key .. "-toggle", checked = checked, onChange = onChange }),
    ui.label({ text = label, flexGrow = 1 }),
  })
end

local function setBool(key)
  return function(value)
    send({ cmd = "set", key = key, value = value == "true" })
  end
end

local function batteryCell(key, glyphName, label, level, threshold)
  local text = level == nil and "--" or (tostring(level) .. "%")
  local low = level ~= nil and level <= threshold
  return ui.column({ key = key, gap = 4, align = "center" }, {
    ui.row({ align = "center", gap = 4 }, {
      ui.glyph({ name = glyphName, size = 16, color = level == nil and "outline_variant" or "primary" }),
      ui.label({ text = label, fontSize = 12, color = "on_surface_variant" }),
    }),
    ui.label({
      text = text,
      fontSize = 16,
      fontWeight = "bold",
      textAlign = "center",
      color = low and "error" or "on_surface",
    }),
    ui.progress({
      progress = level == nil and 0 or (level / 100),
      width = 72,
      height = 4,
      radius = 2,
      fill = low and "error" or "primary",
    }),
  })
end

local function infoLine(key, label, value)
  if type(value) ~= "string" or value == "" then
    return nil
  end
  return ui.row({ key = key, gap = 8, align = "center" }, {
    ui.label({ text = label, fontSize = 12, color = "on_surface_variant" }),
    ui.label({ text = value, fontSize = 12, flexGrow = 1, maxLines = 2 }),
  })
end

local function peerCard(peer)
  local mac = peer.mac
  if type(mac) ~= "string" or mac == "" then
    return nil
  end
  local name = type(peer.name) == "string" and peer.name or ""
  local flags = {}
  if peer.connected == true then
    table.insert(flags, noctalia.tr("panel.connected"))
  end
  if peer.playing == true then
    table.insert(flags, noctalia.tr("panel.playing"))
  end
  if peer.preferred == true then
    table.insert(flags, noctalia.tr("panel.preferred"))
  end
  if peer.auto_connect == true then
    table.insert(flags, noctalia.tr("panel.auto_on"))
  end
  local buttons = {
    ui.button({
      key = "link-" .. mac,
      text = peer.connected == true and noctalia.tr("panel.disconnect") or noctalia.tr("panel.connect"),
      variant = "outline",
      controlSize = "sm",
      onClick = function()
        send({
          cmd = "dual",
          action = peer.connected == true and "disconnect" or "connect",
          mac = mac,
        })
      end,
    }),
  }
  if peer.preferred ~= true then
    table.insert(buttons, ui.button({
      key = "prefer-" .. mac,
      text = noctalia.tr("panel.prefer"),
      variant = "ghost",
      controlSize = "sm",
      onClick = function()
        send({ cmd = "dual", action = "prefer", mac = mac })
      end,
    }))
  end
  if type(peer.auto_connect) == "boolean" then
    table.insert(buttons, ui.button({
      key = "auto-" .. mac,
      text = noctalia.tr("panel.auto"),
      variant = "ghost",
      controlSize = "sm",
      onClick = function()
        send({ cmd = "dual", action = "auto", mac = mac, value = peer.auto_connect ~= true })
      end,
    }))
  end
  table.insert(buttons, ui.button({
    key = "unpair-" .. mac,
    text = noctalia.tr("panel.unpair"),
    variant = "destructive",
    controlSize = "sm",
    onClick = function()
      send({ cmd = "dual", action = "unpair", mac = mac })
    end,
  }))
  local lines = {
    ui.label({
      text = name ~= "" and name or mac,
      fontWeight = "medium",
      maxLines = 1,
    }),
  }
  if name ~= "" then
    table.insert(lines, ui.label({ text = mac, fontSize = 11, color = "on_surface_variant", maxLines = 1 }))
  end
  if #flags > 0 then
    table.insert(lines, ui.label({
      text = table.concat(flags, " · "),
      fontSize = 11,
      color = "on_surface_variant",
      maxLines = 2,
    }))
  end
  return ui.column({
    key = "peer-" .. mac,
    gap = 8,
    padding = 10,
    radius = 10,
    align = "stretch",
    fill = peer.connected == true and "primary/0.12" or "on_surface/0.06",
  }, {
    ui.column({ gap = 2, align = "stretch" }, lines),
    ui.row({ gap = 6, align = "center" }, buttons),
  })
end

local function refreshVolume()
  if polling or not noctalia.commandExists("wpctl") then
    return
  end
  polling = true
  noctalia.runAsync({ "wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@" }, function(result)
    polling = false
    if not open then
      return
    end
    local value = string.match(result.stdout or "", "Volume:%s*([%d%.]+)")
    if result.exitCode == 0 and value ~= nil then
      volume = math.max(0, math.min(100, math.floor(((tonumber(value) or 0) * 100) + 0.5)))
      muted = string.find(result.stdout or "", "MUTED") ~= nil
      volumeKnown = true
      render()
    end
  end, 3000)
end

local function openDesktop(page)
  noctalia.runAsync({ "noctalia", "msg", "panel-open", "control-center", page })
end

render = function()
  local snap = snapshot()
  local ready = snap.spp_connected == true
  local threshold = math.floor(tonumber(noctalia.getConfig("low_battery_percent")) or 15)
  local battery = type(snap.battery) == "table" and snap.battery or {}
  local info = type(snap.info) == "table" and snap.info or {}
  local gestures = type(snap.gestures) == "table" and snap.gestures or {}
  local gestureOptions = type(snap.gesture_options) == "table" and snap.gesture_options or {}
  local dual = type(snap.dual_connect) == "table" and snap.dual_connect or {}
  local body = {}

  local err = labels.error(snap.error)
  if err ~= nil then
    table.insert(body, ui.row({
      key = "error",
      gap = 8,
      padding = 10,
      radius = 10,
      align = "center",
      fill = "error/0.12",
    }, {
      ui.label({ text = err, flexGrow = 1, maxLines = 4, color = "error" }),
    }))
  end

  local status = {}
  local function addStatus(row)
    if row ~= nil then
      table.insert(status, row)
    end
  end
  addStatus(infoLine("phase", noctalia.tr("panel.phase"), labels.phase(snap.phase)))
  addStatus(infoLine("address", noctalia.tr("panel.address"), snap.address))
  addStatus(infoLine(
    "audio",
    noctalia.tr("panel.audio"),
    snap.audio_connected == true and noctalia.tr("panel.connected") or noctalia.tr("panel.offline")
  ))
  if ready then
    local wearing = noctalia.tr("panel.wearing_unknown")
    if snap.wearing == true then
      wearing = noctalia.tr("panel.wearing_on")
    elseif snap.wearing == false then
      wearing = noctalia.tr("panel.wearing_off")
    end
    addStatus(infoLine("wearing", noctalia.tr("panel.wearing"), wearing))
  end
  table.insert(body, ui.column({ key = "status", gap = 4, align = "stretch" }, status))

  local left = percent(battery.left)
  local right = percent(battery.right)
  local caseLevel = percent(battery.case)
  if left ~= nil or right ~= nil or caseLevel ~= nil then
    local cells = {
      batteryCell("bat-left", "ear", noctalia.tr("panel.left"), left, threshold),
      batteryCell("bat-right", "ear", noctalia.tr("panel.right"), right, threshold),
      batteryCell("bat-case", "battery", noctalia.tr("panel.case"), caseLevel, threshold),
    }
    local batteryBlock = {
      ui.row({ align = "center", justify = "space_between" }, cells),
    }
    if battery.charging == true then
      table.insert(batteryBlock, ui.label({
        text = noctalia.tr("panel.charging"),
        fontSize = 12,
        color = "secondary",
        textAlign = "center",
      }))
    end
    table.insert(body, ui.column({ key = "battery", gap = 6, align = "stretch" }, batteryBlock))
  end

  if ready then
    if type(snap.low_latency) == "boolean" then
      table.insert(body, toggleRow("latency", noctalia.tr("panel.latency"), snap.low_latency == true, setBool("low_latency")))
    end
    if type(snap.auto_pause) == "boolean" then
      table.insert(body, toggleRow("auto-pause", noctalia.tr("panel.auto_pause"), snap.auto_pause == true, setBool("auto_pause")))
    end
    if type(dual.enabled) == "boolean" then
      table.insert(body, toggleRow("dual-enabled", noctalia.tr("panel.dual_enabled"), dual.enabled == true, setBool("dual_connect")))
    end
    if type(snap.sound_quality) == "string" then
      table.insert(body, section(
        "quality",
        noctalia.tr("panel.quality"),
        choiceSelect("quality-select", withCurrent(QUALITY, snap.sound_quality), snap.sound_quality, labels.quality, function(choice)
          send({ cmd = "set", key = "sound_quality", value = choice })
        end)
      ))
    end
    if type(snap.equalizer) == "string" then
      local eqIds = withCurrent(snap.equalizer_options, snap.equalizer)
      if #eqIds == 0 then
        eqIds = withCurrent(EQ_FALLBACK, snap.equalizer)
      end
      table.insert(body, section(
        "equalizer",
        noctalia.tr("panel.equalizer"),
        choiceSelect("eq-select", eqIds, snap.equalizer, labels.eq, function(choice)
          send({ cmd = "set", key = "equalizer", value = choice })
        end)
      ))
    end

    local gestureRows = {}
    for _, spec in ipairs(GESTURES) do
      local current = gestures[spec.id]
      local ids = withCurrent(gestureOptions[spec.options], current)
      if #ids > 0 then
        table.insert(gestureRows, section(
          spec.id,
          noctalia.tr(spec.label),
          choiceSelect(spec.id .. "-select", ids, current, labels.action, function(choice)
            send({ cmd = "set", key = "gesture", gesture = spec.id, value = choice })
          end)
        ))
      end
    end
    if #gestureRows > 0 then
      table.insert(body, ui.column({ key = "gestures", gap = 10, align = "stretch" }, {
        ui.label({ text = noctalia.tr("panel.gestures"), fontWeight = "bold" }),
        ui.column({ gap = 10, align = "stretch" }, gestureRows),
      }))
    end

    local locales = strings(snap.language_options)
    if #locales > 0 then
      table.insert(body, section(
        "language",
        noctalia.tr("panel.language"),
        choiceSelect("language-select", locales, snap.language, function(id)
          return id
        end, function(choice)
          send({ cmd = "set", key = "language", value = choice })
        end)
      ))
    end

    local devices = type(dual.devices) == "table" and dual.devices or {}
    local cards = {}
    for _, peer in ipairs(devices) do
      if type(peer) == "table" then
        local card = peerCard(peer)
        if card ~= nil then
          table.insert(cards, card)
        end
      end
    end
    if #cards > 0 or type(dual.enabled) == "boolean" then
      if #cards == 0 then
        table.insert(cards, ui.label({
          text = noctalia.tr("panel.no_devices"),
          fontSize = 12,
          color = "on_surface_variant",
        }))
      end
      table.insert(body, ui.column({ key = "dual", gap = 8, align = "stretch" }, {
        ui.label({ text = noctalia.tr("panel.dual"), fontWeight = "bold" }),
        ui.column({ gap = 8, align = "stretch" }, cards),
      }))
    end
  elseif snap.phase ~= "starting" and snap.phase ~= "connecting" and snap.phase ~= "reconnecting" then
    table.insert(body, ui.column({ key = "offline", gap = 8, align = "center", padding = 12 }, {
      ui.glyph({ name = "headphones-off", size = 36, color = "outline_variant" }),
      ui.label({
        text = noctalia.tr("panel.not_connected"),
        fontWeight = "medium",
        textAlign = "center",
        maxLines = 3,
      }),
    }))
  end

  if noctalia.commandExists("wpctl") then
    if volumeKnown then
      table.insert(body, section(
        "volume",
        noctalia.tr("panel.volume") .. " " .. tostring(volume) .. "%",
        ui.column({ gap = 8, align = "stretch" }, {
          ui.slider({
            key = "volume-slider",
            min = 0,
            max = 100,
            step = 1,
            value = volume,
            onChange = function(value)
              volume = math.floor(tonumber(value) or 0)
              noctalia.runAsync({ "noctalia", "msg", "volume-set", tostring(volume) })
            end,
            onDragEnd = function()
              if open then
                render()
              end
            end,
          }),
          ui.button({
            key = "mute",
            text = muted and noctalia.tr("panel.unmute") or noctalia.tr("panel.mute"),
            glyph = "volume",
            variant = "outline",
            controlSize = "sm",
            onClick = function()
              muted = not muted
              noctalia.runAsync({ "noctalia", "msg", "volume-mute" })
              if open then
                render()
              end
            end,
          }),
        })
      ))
    else
      table.insert(body, ui.label({
        key = "volume-hint",
        text = noctalia.tr("panel.volume_hint"),
        fontSize = 12,
        color = "on_surface_variant",
      }))
    end
  end

  table.insert(body, ui.row({ key = "desktop", gap = 8, align = "center" }, {
    ui.button({
      key = "bluetooth",
      text = noctalia.tr("panel.bluetooth"),
      glyph = "bluetooth",
      variant = "outline",
      controlSize = "sm",
      flexGrow = 1,
      onClick = function()
        openDesktop("bluetooth")
      end,
    }),
    ui.button({
      key = "desktop-audio",
      text = noctalia.tr("panel.audio"),
      glyph = "volume",
      variant = "outline",
      controlSize = "sm",
      flexGrow = 1,
      onClick = function()
        openDesktop("audio")
      end,
    }),
  }))

  local details = {}
  local detail = infoLine("model", noctalia.tr("panel.model"), info.model)
  if detail ~= nil then
    table.insert(details, detail)
  end
  detail = infoLine("hardware", noctalia.tr("panel.hardware"), info.hardware)
  if detail ~= nil then
    table.insert(details, detail)
  end
  detail = infoLine("firmware", noctalia.tr("panel.firmware"), info.firmware)
  if detail ~= nil then
    table.insert(details, detail)
  end
  detail = infoLine("serial", noctalia.tr("panel.serial"), info.serial)
  if detail ~= nil then
    table.insert(details, detail)
  end
  if #details > 0 then
    table.insert(body, ui.column({ key = "details", gap = 4, align = "stretch" }, details))
  end

  local header = {
    ui.column({ gap = 2, flexGrow = 1, align = "stretch" }, {
      ui.label({
        text = noctalia.tr("panel.title"),
        fontSize = 16,
        fontWeight = "bold",
        maxLines = 1,
      }),
      ui.label({
        text = (type(snap.name) == "string" and snap.name ~= "" and snap.name or "FreeClip 2")
          .. " · "
          .. labels.phase(snap.phase),
        fontSize = 12,
        color = ready and "primary" or "on_surface_variant",
        maxLines = 1,
      }),
    }),
    ui.button({
      key = "refresh",
      glyph = "refresh",
      variant = "ghost",
      controlSize = "sm",
      tooltip = noctalia.tr("panel.refresh"),
      onClick = function()
        send({ cmd = "refresh" })
      end,
    }),
  }
  if closeButton then
    table.insert(header, ui.button({
      key = "close",
      glyph = "close",
      variant = "ghost",
      controlSize = "sm",
      tooltip = noctalia.tr("panel.close"),
      onClick = function()
        panel.close()
      end,
    }))
  end

  panel.render(ui.column({ flexGrow = 1, gap = 12, align = "stretch" }, {
    ui.row({ align = "center", justify = "space_between", gap = 8 }, header),
    ui.scroll({ flexGrow = 1, gap = 14, align = "stretch" }, body),
  }))
end

noctalia.state.watch("snapshot", function()
  if open then
    render()
  end
end)

function onOpen(_context)
  open = true
  closeButton = noctalia.getSetting("shell.show_close_buttons") ~= false
  noctalia.setUpdateInterval(4000)
  refreshVolume()
  render()
end

function onClose()
  open = false
  noctalia.setUpdateInterval(60000)
end

function update()
  if open then
    refreshVolume()
  end
end

noctalia.setUpdateInterval(60000)
