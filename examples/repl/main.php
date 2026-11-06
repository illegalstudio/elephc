<?php
// Load this file with require_once 'examples/repl/main.php' in elephc repl.
function totalWithTax(float $price, float $rate): float {
    return $price * (1 + $rate);
}
