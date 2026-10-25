<?php

namespace App\Http\Controllers;

use App\Models\Order;
use App\Services\Pricing;

class OrderController
{
    public function show(): string
    {
        $order = new Order();
        return Pricing::format($order->total());
    }
}
