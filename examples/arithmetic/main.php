<?php
$a = 10;
$b = 32;

echo "a = " . $a . ", b = " . $b . "\n";
echo "a + b = " . ($a + $b) . "\n";
echo "a - b = " . ($a - $b) . "\n";
echo "a * b = " . ($a * $b) . "\n";
echo "b / a = " . intval($b / $a) . "\n";
echo "b % a = " . ($b % $a) . "\n";
echo "2 + 3 * 4 = " . (2 + 3 * 4) . "\n";
echo "(2 + 3) * 4 = " . ((2 + 3) * 4) . "\n";

$overflow = PHP_INT_MAX + $argc;
echo "overflow type = " . gettype($overflow) . "\n";
