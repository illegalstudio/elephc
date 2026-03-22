<?php
// Array operations

$numbers = [64, 25, 12, 89, 37, 41];
echo "Numbers: ";
foreach ($numbers as $n) {
    echo $n . " ";
}
echo "\n";
echo "Count: " . count($numbers) . "\n";

// Find max
function find_max($arr) {
    $max = $arr[0];
    $i = 1;
    while ($i < count($arr)) {
        if ($arr[$i] > $max) {
            $max = $arr[$i];
        }
        $i++;
    }
    return $max;
}

echo "Max: " . find_max($numbers) . "\n";

// Sum
function sum($arr) {
    $total = 0;
    foreach ($arr as $v) {
        $total += $v;
    }
    return $total;
}

echo "Sum: " . sum($numbers) . "\n";

// Build array dynamically
$squares = [1];
for ($i = 2; $i <= 5; $i++) {
    $squares[] = $i * $i;
}

echo "Squares: ";
foreach ($squares as $s) {
    echo $s . " ";
}
echo "\n";

// String array
$langs = ["PHP", "Rust", "ARM64"];
echo "Compiled " . $langs[0] . " to " . $langs[2] . " with " . $langs[1] . "\n";
