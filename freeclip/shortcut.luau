--!nonstrict
-- Control-center tile for low-latency mode. Right click opens the panel.

local client = require("./lib/client.luau")

local PANEL_ID = "zigzag/freeclip:panel"

local function render(snap)
  if type(snap) ~= "table" then
    snap = {}
  end
  local known = snap.spp_connected == true and type(snap.low_latency) == "boolean"
  local enabled = snap.low_latency == true
  shortcut.setLabel(noctalia.tr(enabled and "shortcut.on" or "shortcut.off"))
  shortcut.setIcon("bolt")
  shortcut.setActive(enabled)
  shortcut.setEnabled(known)
end

noctalia.state.watch("snapshot", render)
render(noctalia.state.get("snapshot"))

function onClick()
  local snap = noctalia.state.get("snapshot") or {}
  if snap.spp_connected ~= true or type(snap.low_latency) ~= "boolean" then
    return
  end
  client.send({ cmd = "set", key = "low_latency", value = not snap.low_latency }, function(ack)
    if ack.ok == false then
      noctalia.notifyError(noctalia.tr("notify.command_failed"), tostring(ack.error or ""))
    end
  end)
end

function onRightClick()
  noctalia.togglePanel(PANEL_ID)
end
