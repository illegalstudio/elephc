<?php
function mapLocal(int $limit): float {
    $values = range(1, 100);
    $sum = 0;
    for ($i = 0; $i < 4999 + $limit; $i++) {
        $mapped = array_map(fn($v) => $v + 1.5, $values);
        $sum += array_sum($mapped);
    }
    return $sum;
}
echo mapLocal($argc), "\n";
