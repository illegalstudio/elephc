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

// Numeric strings are coerced at runtime, matching PHP semantics.
echo '"123" + 3 = ' . ("123" + 3) . "\n";
echo '"1.5" + 3 = ' . ("1.5" + 3) . "\n";
echo '"100" - 30 = ' . ("100" - 30) . "\n";
echo '"10" / 3 = ' . ("10" / 3) . "\n";
