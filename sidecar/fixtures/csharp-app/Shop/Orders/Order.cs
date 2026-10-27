namespace Shop.Orders;

public class Order
{
    private readonly int _count;

    public Order(int count) => _count = count;

    public static Order Create() => new Order(1);

    public int Total() => _count * Pricing.Rate();
}
