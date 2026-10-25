package main

import (
	"fmt"

	"example.com/shop/order"
)

func main() {
	o := order.New()
	fmt.Println(o.Total())
}
