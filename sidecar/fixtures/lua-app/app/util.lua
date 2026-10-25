local M = {}

function M.clamp(value, low, high)
  return math.max(low, math.min(high, value))
end

return M
