package com.shop;

import com.shop.order.Order;

public class Main {
    public static void main(String[] args) {
        Order order = Order.create();
        System.out.println(order.total());
    }
}
