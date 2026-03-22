<?php
// Functions: declaration, return, recursion, composition

function abs_val($x) {
    if ($x < 0) {
        return -$x;
    }
    return $x;
}

function max($a, $b) {
    if ($a > $b) {
        return $a;
    }
    return $b;
}

function min($a, $b) {
    if ($a < $b) {
        return $a;
    }
    return $b;
}

function clamp($val, $lo, $hi) {
    return max($lo, min($val, $hi));
}

function gcd($a, $b) {
    $a = abs_val($a);
    $b = abs_val($b);
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

echo "abs(-42) = " . abs_val(-42) . "\n";
echo "max(3, 7) = " . max(3, 7) . "\n";
echo "clamp(15, 0, 10) = " . clamp(15, 0, 10) . "\n";
echo "gcd(48, 18) = " . gcd(48, 18) . "\n";
echo "2^10 = " . power(2, 10) . "\n";
