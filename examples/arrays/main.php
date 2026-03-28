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

// List unpacking
[$first, $second] = [$numbers[0], $numbers[1]];
echo "First two: " . $first . ", " . $second . "\n";

// array_column on associative rows
$users = [
    ["name" => "Ada", "score" => 10],
    ["name" => "Linus", "score" => 12],
    ["name" => "Grace", "score" => 8],
];
$names = array_column($users, "name");
echo "Names: ";
foreach ($names as $name) {
    echo $name . " ";
}
echo "\n";

// String array
$langs = ["PHP", "Rust", "ARM64"];
echo "Compiled " . $langs[0] . " to " . $langs[2] . " with " . $langs[1] . "\n";
