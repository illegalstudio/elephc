<?php

// Callback-based array functions demo

function double($x) { return $x * 2; }
function is_positive($x) { return $x > 0; }
function sum($carry, $item) { return $carry + $item; }
function compare($a, $b) { return $a - $b; }
function show($x) { echo "  " . $x . "\n"; }

$numbers = [3, -1, 4, -5, 2, -3, 1];

// array_map: transform each element
$doubled = array_map("double", $numbers);
echo "Doubled: ";
foreach ($doubled as $v) { echo $v . " "; }
echo "\n";

// array_filter: keep only matching elements
$positives = array_filter($numbers, "is_positive");
echo "Positives: ";
foreach ($positives as $v) { echo $v . " "; }
echo "\n";

// array_reduce: fold into a single value
$total = array_reduce($numbers, "sum", 0);
echo "Sum: " . $total . "\n";

// usort: sort with custom comparator
$sorted = [5, 2, 8, 1, 9];
usort($sorted, "compare");
echo "Sorted: ";
foreach ($sorted as $v) { echo $v . " "; }
echo "\n";

// array_walk: apply side-effect to each element
echo "Walk:\n";
$items = [10, 20, 30];
array_walk($items, "show");

// call_user_func: indirect function call
$result = call_user_func("double", 21);
echo "call_user_func(double, 21) = " . $result . "\n";

// function_exists: check if a function is defined
if (function_exists("double")) {
    echo "function 'double' exists\n";
}
if (!function_exists("nonexistent")) {
    echo "function 'nonexistent' does not exist\n";
}
