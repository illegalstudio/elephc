<?php
// Integer overflow promotes this accumulator to float for all later iterations.
function promoted_total(int $iterations): mixed {
    $total = PHP_INT_MAX - 1;
    for ($i = 0; $i < $iterations; $i++) {
        $total += 1;
    }
    return $total;
}
$value = promoted_total($argc + 2);
echo gettype($value), ':', @(int)$value, "\n";
