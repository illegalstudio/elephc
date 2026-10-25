<?php
// Compares every pair of points once: $i stops one short of the end, and $j starts just
// after $i. Both counters run as raw integers. `$n - 1` and `$i + 1` are checked
// arithmetic that PHP could turn into floats, but the loop bounds prove they never do here.
function closestPair(array $xs, array $ys): string {
    $n = count($xs);
    $best = PHP_INT_MAX;
    $pair = '';
    for ($i = 0; $i < $n - 1; $i++) {
        for ($j = $i + 1; $j < $n; $j++) {
            $dx = $xs[$i] - $xs[$j];
            $dy = $ys[$i] - $ys[$j];
            $distance = $dx * $dx + $dy * $dy;
            if ($distance < $best) {
                $best = $distance;
                $pair = $i . '-' . $j;
            }
        }
    }
    return $pair . ' at ' . $best;
}

// Prints: 1-3 at 5
echo closestPair([0, 7, 3, 8, 14], [0, 1, 9, 3, 2]), "\n";
