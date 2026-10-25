<?php

namespace App\Models;

class Order
{
    public function total(): int
    {
        return $this->sum();
    }

    private function sum(): int
    {
        return 1;
    }
}
