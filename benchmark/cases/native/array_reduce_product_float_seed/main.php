<?php
$values = range(1, 10);
$sum = 0;
for ($i = 0; $i < 4999 + $argc; $i++) {
    $sum += array_reduce($values, fn($c, $v) => $c * $v, 1.5);
}
echo $sum, "\n";
