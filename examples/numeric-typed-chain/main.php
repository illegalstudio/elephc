<?php
// Checked integer arithmetic promotes to float on overflow before validating the return type.
function weightedTotal(int $count, int $weight): int {
    $total = 0;
    for ($index = 0; $index < $count; $index++) {
        $total = $total + $index * $weight;
    }
    return $total;
}

echo weightedTotal($argc * 1000, 3);
