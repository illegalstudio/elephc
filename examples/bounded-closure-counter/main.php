<?php
function closure_total(int $n): int {
    // Exact integer arguments let the compiler avoid per-call Mixed argument copies.
    $next = function ($x) { return $x + 1; };
    $total = 0;
    for ($i = 0; $i < $n; $i++) {
        $total += $next($i);
    }
    return (int)$total;
}
echo closure_total($argc * 1000);
