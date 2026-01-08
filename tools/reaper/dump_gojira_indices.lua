-- Dumps Neural DSP Gojira parameter names around suspect indices (101/105/112/114).
-- Run via REAPER: Actions -> Show action list -> ReaScript -> Load, then run.

local function join_path(a, b)
  if a:sub(-1) == "\\" or a:sub(-1) == "/" then return a .. b end
  return a .. "\\" .. b
end

local function write_all(path, text)
  local f = io.open(path, "wb")
  if not f then return false end
  f:write(text)
  f:close()
  return true
end

local function lower(s)
  return string.lower(s or "")
end

local resource = reaper.GetResourcePath()
local out_path = join_path(resource, "codex_gojira_param_dump.txt")

local lines = {}
local function log(s)
  lines[#lines + 1] = s
end

log("REAPER: " .. tostring(reaper.GetAppVersion()))
log("Project: " .. tostring(reaper.GetProjectPath("")))
log("---")

local found = 0
for ti = 0, reaper.CountTracks(0) - 1 do
  local tr = reaper.GetTrack(0, ti)
  local fxCount = reaper.TrackFX_GetCount(tr)
  for fi = 0, fxCount - 1 do
    local _, fxName = reaper.TrackFX_GetFXName(tr, fi, "")
    fxName = fxName or ""
    if lower(fxName):find("gojira") then
      found = found + 1
      log(string.format("Track %d FX %d: %s", ti, fi, fxName))

      local important = { 101, 105, 112, 114 }
      for _, idx in ipairs(important) do
        local ok, pName = reaper.TrackFX_GetParamName(tr, fi, idx, "")
        pName = pName or ""
        if ok then
          log(string.format("  idx %d: %s", idx, pName))
        else
          log(string.format("  idx %d: <no name>", idx))
        end
      end

      log("  window 90..130:")
      for idx = 90, 130 do
        local ok, pName = reaper.TrackFX_GetParamName(tr, fi, idx, "")
        pName = pName or ""
        if ok and pName ~= "" then
          log(string.format("    %d: %s", idx, pName))
        end
      end

      log("---")
    end
  end
end

if found == 0 then
  log('No FX with name containing "gojira" found in the current project.')
end

local ok = write_all(out_path, table.concat(lines, "\r\n") .. "\r\n")
if ok then
  reaper.ShowConsoleMsg("Wrote " .. out_path .. "\n")
else
  reaper.ShowConsoleMsg("Failed to write " .. out_path .. "\n")
end

