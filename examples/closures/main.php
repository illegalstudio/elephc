<?php

// Anonymous functions (closures) and arrow functions

// Basic anonymous function assigned to a variable
$double = function($x) { return $x * 2; };
echo "double(5) = ";
echo $double(5);
echo "\n";

// Arrow function (shorthand syntax)
$triple = fn($x) => $x * 3;
echo "triple(4) = ";
echo $triple(4);
echo "\n";

// Multi-parameter closure
$add = function($a, $b) { return $a + $b; };
echo "add(3, 7) = ";
echo $add(3, 7);
echo "\n";

// Arrow function with expression body
$square_plus_one = fn($x) => $x * $x + 1;
echo "square_plus_one(5) = ";
echo $square_plus_one(5);
echo "\n";

// Closures as callbacks to array_map
$values = [1, 2, 3, 4];
$doubled = array_map(fn($x) => $x * 2, $values);
echo "doubled: ";
echo $doubled[0];
echo " ";
echo $doubled[1];
echo " ";
echo $doubled[2];
echo " ";
echo $doubled[3];
echo "\n";

// Closures as callbacks to array_filter
$numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
$evens = array_filter($numbers, fn($n) => $n % 2 == 0);
echo "Even count: ";
echo count($evens);
echo "\n";

// Closures with array_reduce
$sum = array_reduce([1, 2, 3, 4, 5], function($carry, $item) {
    return $carry + $item;
}, 0);
echo "Sum of 1..5 = ";
echo $sum;
echo "\n";
