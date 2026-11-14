<?php
// Partial function application is PHP 8.6 syntax: compile with --php-version=8.6.
function formatPrice(float $amount, string $symbol, int $decimals = 2): string
{
    return $symbol . number_format($amount, $decimals);
}

$euros = formatPrice(?, '€');
$dollars = formatPrice(?, '$', 0);

foreach ([12.5, 3.25] as $price) {
    echo $euros($price), ' / ', $dollars($price), PHP_EOL;
}

$names = ['ada', 'grace', 'linus'];
$upper = array_map(strtoupper(?), $names);
echo implode(', ', $upper), PHP_EOL;
