<?php
// Functions: declaration, return, recursion, composition

function my_abs($x) {
    if ($x < 0) {
        return -$x;
    }
    return $x;
}

function my_max($a, $b) {
    if ($a > $b) {
        return $a;
    }
    return $b;
}

function my_min($a, $b) {
    if ($a < $b) {
        return $a;
    }
    return $b;
}

function clamp($val, $lo, $hi) {
    return my_max($lo, my_min($val, $hi));
}

function gcd($a, $b) {
    $a = my_abs($a);
    $b = my_abs($b);
    while ($b != 0) {
        $t = $b;
        $b = $a % $b;
        $a = $t;
    }
    return $a;
}

function power($base, $exp) {
    $result = 1;
    for ($i = 0; $i < $exp; $i++) {
        $result *= $base;
    }
    return $result;
}

echo "my_abs(-42) = " . my_abs(-42) . "\n";
echo "my_max(3, 7) = " . my_max(3, 7) . "\n";
echo "clamp(15, 0, 10) = " . clamp(15, 0, 10) . "\n";
echo "gcd(48, 18) = " . gcd(48, 18) . "\n";
echo "2^10 = " . power(2, 10) . "\n";
