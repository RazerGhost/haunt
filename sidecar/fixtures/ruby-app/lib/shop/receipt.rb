require_relative "order"

module Shop
  class Receipt
    def initialize(order)
      @order = order
    end

    def lines
      [1, 2]
    end

    def text
      count = lines.size
      body = lines.map do |line|
        format_line(line) + footer
      end
      header + body.join + count.to_s
    end

    def summary
      total = footer
      footer = total * 2
      footer + total
    end

    private

    def header
      "Receipt"
    end

    def footer
      "-"
    end

    def format_line(line)
      line.to_s
    end
  end
end
