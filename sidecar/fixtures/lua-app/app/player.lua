local Player = {}
Player.__index = Player

function Player.new(name)
  return setmetatable({ name = name }, Player)
end

function Player:greet()
  return self:format()
end

function Player:format()
  return "hi " .. self.name
end

return Player
