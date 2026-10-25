<?php

use App\Http\Controllers\OrderController;

Route::get('/', [OrderController::class, 'show']);
