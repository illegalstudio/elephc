<?php
function fib($n) {
    if ($n <= 1) {
        return $n;
    }
    return fib($n - 1) + fib($n - 2);
}

echo "Fibonacci sequence:\n";
for ($i = 0; $i <= 20; $i++) {
    echo "fib(" . $i . ") = " . fib($i) . "\n";
}
