import shop.pricing
from shop.order import Order


def main():
    order = Order()
    return order.total() + shop.pricing.rate()
