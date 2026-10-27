require "json"
require_relative "pricing"

module Shop
  class Order
    def self.make
      new(1)
    end

    def initialize(count)
      @count = count
    end

    def total
      round(@count * Pricing.rate)
    end

    private

    def round(value)
      value.round
    end
  end
end
