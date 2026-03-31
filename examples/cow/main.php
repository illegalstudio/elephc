<?php

// Copy-on-write arrays: assigning by value shares storage until the first write.
$left = [1, 2, 3];
$right = $left;

$right[0] = 99;
$right[] = 4;

echo "left: ";
foreach ($left as $value) {
    echo $value . " ";
}
echo "\n";

echo "right: ";
foreach ($right as $value) {
    echo $value . " ";
}
echo "\n";

// Nested arrays still split lazily when the nested container itself is mutated.
$outerA = [[10, 20], [30, 40]];
$outerB = $outerA;
$inner = $outerB[0];
$inner[1] = 77;
$outerB[0] = $inner;

echo "outerA inner: ";
foreach ($outerA[0] as $value) {
    echo $value . " ";
}
echo "\n";

echo "outerB inner: ";
foreach ($outerB[0] as $value) {
    echo $value . " ";
}
echo "\n";
