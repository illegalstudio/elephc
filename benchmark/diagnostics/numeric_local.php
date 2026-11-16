<?php
// Pass the iteration count at runtime to measure a live numeric accumulator.
function numeric_local_sum(int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += $i;
    }
    return (int)$sum;
}
$n = (int)$argv[1];
$start = hrtime(true);
$sum = numeric_local_sum($n);
$elapsed = hrtime(true) - $start;
echo $sum, '|', $elapsed;
