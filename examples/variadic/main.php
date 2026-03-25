<?php

// Variadic functions and spread operator

// A function that accepts any number of integer arguments
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}

echo "sum(1, 2, 3) = " . sum(1, 2, 3) . "\n";
echo "sum(10, 20, 30, 40, 50) = " . sum(10, 20, 30, 40, 50) . "\n";
echo "sum() = " . sum() . "\n";

// Variadic with regular parameters
function log_message($level, ...$parts) {
    echo "[" . $level . "] ";
    foreach ($parts as $part) {
        echo $part . " ";
    }
    echo "\n";
}

log_message("INFO", "Server", "started", "on", "port", "8080");
log_message("ERROR", "Connection", "refused");

// Spread operator: pass array elements as individual arguments
$numbers = [5, 10, 15, 20];
echo "sum(...[5,10,15,20]) = " . sum(...$numbers) . "\n";

// Spread in array literals: merge arrays
$first = [1, 2, 3];
$second = [4, 5, 6];
$merged = [...$first, ...$second];
echo "merged: ";
foreach ($merged as $v) {
    echo $v . " ";
}
echo "\n";

// Mix spread with regular elements
$base = [10, 20];
$extended = [...$base, 30, 40, ...$base];
echo "extended (" . count($extended) . " items): ";
foreach ($extended as $v) {
    echo $v . " ";
}
echo "\n";
