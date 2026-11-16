<?php
// Time a function-local numeric closure loop without eval or process startup.
function closure_counter(int $n): int {
    $next = function ($x) { return $x + 1; };
    $total = 0;
    for ($i = 0; $i < $n; $i++) {
        $total += $next($i);
    }
    return (int)$total;
}
$n = (int)$argv[1];
$start = hrtime(true);
$result = closure_counter($n);
$elapsed = hrtime(true) - $start;
echo $result, '|', $elapsed;
