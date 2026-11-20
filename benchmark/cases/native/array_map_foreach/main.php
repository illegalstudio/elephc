<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 4999 + $argc; $i++) {
    foreach (array_map(fn($v) => $v * 3, $values) as $x) {
        $sum += $x;
    }
}
echo $sum, "\n";
