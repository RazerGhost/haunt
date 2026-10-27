using Shop.Orders;

namespace Shop;

public static class Program
{
    public static void Main()
    {
        var order = Order.Create();
        System.Console.WriteLine(order.Total());
    }
}
