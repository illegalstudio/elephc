<?php

// Array destructuring (PHP 7.1+) binds variables from array elements in a single
// statement. elephc supports the bracket form `[$a, $b] = $expr`, keyed forms
// `['k' => $v] = $expr`, nested patterns, holes (skipped slots), and destructuring
// directly inside `foreach` value patterns.

// Positional destructuring: elements are bound in order from index 0.
[$first, $second, $third] = [10, 20, 30];
echo $first . ',' . $second . ',' . $third . "\n";

// Holes skip a slot without binding, but still advance the positional index.
[, $middle,] = [100, 200, 300];
echo $middle . "\n";

// Keyed destructuring picks elements by string key, regardless of order.
['id' => $id, 'name' => $name] = ['name' => 'Ada', 'id' => 7];
echo $id . ':' . $name . "\n";

// Nested patterns destructure arrays within arrays.
[[$a, $b], [$c, $d]] = [[1, 2], [3, 4]];
echo $a . $b . $c . $d . "\n";

// foreach can destructure each element as it iterates.
foreach (['alice' => ['Alice', 30], 'bob' => ['Bob', 25]] as $key => [$who, $age]) {
    echo $key . '=' . $who . '(' . $age . ")\n";
}

// Keyed destructuring directly as the foreach value pattern.
foreach ([['id' => 1, 'name' => 'Alice'], ['id' => 2, 'name' => 'Bob']] as ['id' => $rowId, 'name' => $rowName]) {
    echo $rowId . ':' . $rowName . "\n";
}

// A positional list-destructuring assignment can also be used as an expression: it binds the
// targets and evaluates to the right-hand side. A common idiom is assign-and-test in a condition,
// where the destructured variables are then used in the body.
$rows = [[1, 'one'], [2, 'two'], [3, 'three']];
foreach ($rows as $row) {
    if ([$num, $label] = $row) {
        echo 'row ' . $num . ' = ' . $label . "\n";
    }
}