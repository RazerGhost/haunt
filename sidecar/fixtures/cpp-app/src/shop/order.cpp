#include "order.hpp"
#include "pricing.hpp"

namespace shop {

Order Order::make(int count) {
    return Order(count);
}

int Order::total() const {
    return count_ * pricing::rate();
}

}
