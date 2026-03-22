<?php
// Multi-file example: include functions from other files

require_once 'math.php';
require_once 'greet.php';

hello("World");

echo "3 + 4 = " . add(3, 4) . "\n";
echo "5 * 6 = " . multiply(5, 6) . "\n";
echo "10! = " . factorial(10) . "\n";
