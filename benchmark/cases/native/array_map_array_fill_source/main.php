<?php
$sum = 0;
for ($i = 0; $i < 4999 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 3, array_fill(0, 100, 2));
    $sum += array_sum($mapped);
}
echo $sum, "\n";
