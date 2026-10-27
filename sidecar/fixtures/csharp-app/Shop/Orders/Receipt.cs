namespace Shop.Orders;

public class Receipt
{
    private readonly Order _order;

    public Receipt(Order order) => _order = order;

    public int Lines { get; set; }

    public int Amount => _order.Total() + Fee();

    public string Text
    {
        get { return Format(Amount); }
    }

    private string _note = "";

    public string Note
    {
        get => _note;
        set => _note = Format(value.Length);
    }

    public static Receipt operator +(Receipt a, Receipt b) => Merge(a, b);

    public static implicit operator int(Receipt receipt) => receipt.Amount;

    private static int Fee() => Pricing.Rate();

    private static string Format(int value) => value.ToString();

    private static Receipt Merge(Receipt a, Receipt b) => new Receipt(a._order);
}
