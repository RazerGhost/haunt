require_relative "lib/shop/order"

def main
  order = Shop::Order.make
  puts order.total
end
