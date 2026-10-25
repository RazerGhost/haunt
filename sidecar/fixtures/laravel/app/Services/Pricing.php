<?php

namespace App\Services;

class Pricing
{
    public static function format(int $amount): string
    {
        return helper_round($amount);
    }
}
