<?php
$left = range(1, 50);
$right = range(51, 100);
$sum = 0;
for ($i = 0; $i < 4999 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, array_merge($left, $right));
    $sum += array_sum($mapped);
}
echo $sum, "\n";
