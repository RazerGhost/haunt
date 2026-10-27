#include "shop/order.hpp"

int main() {
    auto order = shop::Order::make(2);
    return order.total();
}
