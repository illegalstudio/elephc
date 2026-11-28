<?php
// Compile with: elephc build --strict examples/strict-mode/main.php
function total(array<int> $values): int|float {
    $sum = 0;
    foreach ($values as $value) {
        $sum += $value;
    }
    return $sum;
}

function integerOrZero(mixed $value): int {
    if (is_int($value)) {
        return $value;
    }
    return 0;
}

$scores = [2, 3];
echo (string) total($scores), PHP_EOL;
echo (string) integerOrZero(7), PHP_EOL;

array<string, int> $counts = ['wins' => 1, 'draws' => 2];
echo (string) count($counts), PHP_EOL;
