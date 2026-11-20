<?php
$values = range(2, 200, 2);
$sum = 0;
for ($i = 0; $i < 4999 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v / 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
