--!nonstrict
-- Resolve freeclipd and send one control command over its Unix socket.

local M = {}

function M.shellQuote(value)
  return "'" .. string.gsub(tostring(value), "'", [['"'"']]) .. "'"
end

function M.binary()
  local configured = noctalia.getConfig("binary_path")
  if type(configured) == "string" and configured ~= "" then
    if string.sub(configured, 1, 1) == "~" then
      return noctalia.expandPath(configured)
    end
    return configured
  end
  local bundled = noctalia.pluginDir() .. "/bin/freeclipd"
  if noctalia.fileExists(bundled) then
    return bundled
  end
  if noctalia.commandExists("freeclipd") then
    return "freeclipd"
  end
  return nil
end

function M.address()
  local address = noctalia.getConfig("device_address")
  if type(address) ~= "string" then
    return nil
  end
  address = string.gsub(address, "^%s*(.-)%s*$", "%1")
  if address == "" then
    return nil
  end
  return address
end

function M.serveCommand()
  local bin = M.binary()
  if bin == nil then
    return nil
  end
  local command = M.shellQuote(bin) .. " serve"
  local address = M.address()
  if address ~= nil then
    command = command .. " --address " .. M.shellQuote(address)
  end
  return command
end

function M.send(request, callback)
  local bin = M.binary()
  if bin == nil then
    if callback ~= nil then
      callback({ ok = false, error = "missing binary" })
    end
    return false
  end
  local payload = noctalia.json.encode(request)
  return noctalia.runAsync({ bin, "ctl", payload }, function(result)
    if callback == nil then
      return
    end
    local stdout = result.stdout or ""
    local ack = noctalia.json.decode(stdout)
    if type(ack) == "table" then
      callback(ack)
      return
    end
    local detail = result.stderr
    if detail == nil or detail == "" then
      detail = "no reply from freeclipd"
    end
    callback({ ok = false, error = detail })
  end, 40000)
end

return M
