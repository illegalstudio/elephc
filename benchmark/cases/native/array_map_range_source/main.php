<?php
$sum = 0;
for ($i = 0; $i < 4999 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v + 1.5, range(1, 100));
    $sum += array_sum($mapped);
}
echo $sum, "\n";
