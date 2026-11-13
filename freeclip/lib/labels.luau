--!nonstrict
-- Translation helpers shared by the widget, panel, and shortcut.

local M = {}

local function translated(key, fallback)
  local text = noctalia.tr(key)
  if text == key then
    return fallback
  end
  return text
end

function M.phase(name)
  if type(name) ~= "string" or name == "" then
    name = "starting"
  end
  return translated("phase." .. name, name)
end

function M.action(name)
  if type(name) ~= "string" or name == "" then
    return ""
  end
  if string.sub(name, 1, 5) == "code:" then
    return name
  end
  return translated("action." .. name, name)
end

function M.eq(name)
  return translated("eq." .. tostring(name), tostring(name))
end

function M.quality(name)
  return translated("quality." .. tostring(name), tostring(name))
end

function M.error(err)
  if err == "binary" then
    return noctalia.tr("error.binary")
  end
  if err == "stream" then
    return noctalia.tr("error.stream")
  end
  if type(err) == "string" and err ~= "" then
    return err
  end
  return nil
end

return M
