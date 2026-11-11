<?php

// Sum 1..$argv[1]. The additions stay in integer registers until one overflows to float.
$n = (int) ($argv[1] ?? 10);
$sum = 0;
$i = 1;
while ($i <= $n) {
    $sum += $i;
    $i += 1;
}
echo $sum, "\n";
