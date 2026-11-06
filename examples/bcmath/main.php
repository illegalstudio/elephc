<?php
use BcMath\Number;

// Exact invoice arithmetic without binary floating-point rounding.

bcscale(2);

$net = '149.99';
$taxRate = '0.22';
$tax = bcmul($net, $taxRate);
$gross = bcadd($net, $tax);

echo "Net:   ", $net, "\n";
echo "Tax:   ", $tax, "\n";
echo "Gross: ", $gross, "\n";

[$whole, $remainder] = bcdivmod($gross, '10');
echo "Ten-euro units: ", $whole, ", remainder: ", $remainder, "\n";

// Immutable decimals choose their own scale and support arithmetic operators.
$price = new Number('149.99');
$rate = new Number('0.22');
$total = $price + ($price * $rate)->round(2);
echo "Object total: ", $total, " (scale ", $total->scale, ")\n";
echo "Above 180: ", $total > 180 ? 'yes' : 'no', "\n";
