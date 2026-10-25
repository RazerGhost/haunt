package order

type Order struct{ count int }

func New() *Order {
	return &Order{count: 1}
}

func (o *Order) Total() int {
	return o.count * rate()
}
