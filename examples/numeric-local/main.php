<?php
// Numeric accumulators keep their value local and expose only a final scalar result.
#[Export]
function local_sum(int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += $i;
    }
    return (int)$sum;
}
echo local_sum($argc * 1000);
