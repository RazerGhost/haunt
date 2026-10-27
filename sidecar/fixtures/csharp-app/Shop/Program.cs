using Shop.Orders;

namespace Shop;

public static class Program
{
    public static void Main()
    {
        var order = Order.Create();
        System.Console.WriteLine(order.Total());
        var receipt = new Receipt(order);
        var both = receipt + receipt;
        int total = (int)receipt;
        receipt.Lines = total;
        receipt.Note = "paid";
        order.Changed += (sender, args) => { };
    }
}
