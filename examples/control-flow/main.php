<?php
// Control flow: if/elseif/else, while, for, break, continue

// Classify a number
function classify($n) {
    if ($n > 0) {
        return "positive";
    } elseif ($n < 0) {
        return "negative";
    } else {
        return "zero";
    }
}

echo classify(42) . ", " . classify(-7) . ", " . classify(0) . "\n";

// Count even numbers with for + continue
$evens = 0;
for ($i = 1; $i <= 20; $i++) {
    if ($i % 2 != 0) {
        continue;
    }
    $evens++;
}
echo "Even numbers 1-20: " . $evens . "\n";

// Find first multiple of 7 with while + break
$n = 1;
while ($n <= 100) {
    if ($n % 7 == 0 && $n > 20) {
        echo "First multiple of 7 above 20: " . $n . "\n";
        break;
    }
    $n++;
}

// Nested loops: multiplication table header
echo "\nMultiplication table (1-5):\n";
for ($row = 1; $row <= 5; $row++) {
    for ($col = 1; $col <= 5; $col++) {
        $product = $row * $col;
        if ($product < 10) {
            echo " ";
        }
        echo $product . " ";
    }
    echo "\n";
}
