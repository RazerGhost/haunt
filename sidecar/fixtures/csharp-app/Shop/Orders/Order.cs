namespace Shop.Orders;

public class Order
{
    private readonly int _count;

    private System.EventHandler? _changed;

    public Order(int count) => _count = count;

    public static Order Create() => new Order(1);

    public int Total() => _count * Pricing.Rate();

    public event System.EventHandler Changed
    {
        add { _changed += value; Log(); }
        remove { _changed -= value; }
    }

    public event System.EventHandler? Closed;

    private static void Log() { }
}
