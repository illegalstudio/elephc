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

// A typed variadic: the element type documents the accepted arguments.
function average(int ...$nums): int {
    $count = count($nums);
    if ($count === 0) {
        return 0;
    }
    return intdiv(array_sum($nums), $count);
}

echo "average(2, 4, 6) = " . average(2, 4, 6) . "\n";

// Typed variadic on a closure.
$concat = function (string ...$parts): string {
    return implode("", $parts);
};
echo "concat = " . $concat("el", "eph", "c") . "\n";

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

function labeled_pair($left, $right) {
    echo "pair: " . $left . ":" . $right . "\n";
}

$named = ["right" => "R", "left" => "L"];
labeled_pair(...$named);

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

// Spread of associative arrays: string keys are preserved, integer keys are reindexed
$config = ['host' => 'localhost', 'port' => 8080];
$defaults = ['host' => '0.0.0.0', 'timeout' => 30];
$merged_config = [...$defaults, ...$config];
foreach ($merged_config as $key => $value) {
    echo $key . "=" . $value . " ";
}
echo "\n";
