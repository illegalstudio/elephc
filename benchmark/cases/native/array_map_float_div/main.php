<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 4999 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v / 2.0, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
