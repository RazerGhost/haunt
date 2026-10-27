package com.shop.order;

public class Order {
    private final int count;

    public Order(int count) {
        this.count = count;
    }

    public static Order create() {
        return new Order(1);
    }

    public int total() {
        return count * Pricing.rate();
    }
}
