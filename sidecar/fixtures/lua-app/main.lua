local util = require("app.util")
local Player = require "app.player"

local function start()
  local p = Player.new("ann")
  p:greet()
  return util.clamp(1, 0, 2)
end

start()
