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
echo "array_sum: " . array_sum($numbers) . "\n";
echo "array_product([2,3,4]): " . array_product([2, 3, 4]) . "\n";

$numbers[2] += 8;
$numbers[3] >>= 1;
echo "Adjusted slots: " . $numbers[2] . ", " . $numbers[3] . "\n";

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

$range = range(3, 7);
echo "Range 3..7: ";
foreach ($range as $value) {
    echo $value . " ";
}
echo "\n";

$slice = array_slice($numbers, 1, 3);
echo "Slice [1,3): ";
foreach ($slice as $value) {
    echo $value . " ";
}
echo "\n";

// unset() removes a key without renumbering: the array keeps its other keys (a hole)
unset($squares[1]);
echo "After unset(squares[1]): ";
foreach ($squares as $k => $v) {
    echo "$k=>$v ";
}
echo "\n";

$reversed = array_reverse($numbers);
echo "Reversed: ";
foreach ($reversed as $value) {
    echo $value . " ";
}
echo "\n";

$padded = array_pad([1, 2], 5, 9);
echo "Padded: ";
foreach ($padded as $value) {
    echo $value . " ";
}
echo "\n";

$dupes = [1, 2, 2, 3, 3, 4];
$unique = array_unique($dupes);
echo "Unique: ";
foreach ($unique as $value) {
    echo $value . " ";
}
echo "\n";

// PHP array union: left numeric keys win, missing right keys are appended
$union = [10, 20] + [99, 88, 77];
echo "Union: ";
foreach ($union as $value) {
    echo $value . " ";
}
echo "\n";

// List unpacking
[$first, $second] = [$numbers[0], $numbers[1]];
echo "First two: " . $first . ", " . $second . "\n";

[$first, , $third] = [$numbers[0], $numbers[1], $numbers[2]];
echo "Skipped middle: " . $first . ", " . $third . "\n";

[[$top_left, $top_right], [$bottom_left, $bottom_right]] = [[1, 2], [3, 4]];
echo "Nested: " . $top_left . $top_right . $bottom_left . $bottom_right . "\n";

["name" => $person, "role" => $role] = ["role" => "admin", "name" => "Ada"];
echo "Keyed: " . $person . " is " . $role . "\n";

$picked = [0];
[$picked[0], $picked[]] = [7, 8];
echo "Targets: " . $picked[0] . ", " . $picked[1] . "\n";

// Heterogeneous indexed arrays widen to mixed payloads
$mixed = [1, "two"];
$mixed[] = true;
echo "Mixed indexed: ";
foreach ($mixed as $value) {
    echo $value . " ";
}
echo "\n";

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
