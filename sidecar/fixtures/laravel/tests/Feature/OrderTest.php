<?php

use App\Models\Order;

class OrderTest
{
    public function test_total(): void
    {
        (new Order())->total();
    }
}
