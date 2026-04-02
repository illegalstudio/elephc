<?php
// Functions: declaration, return, recursion, composition

function my_abs(int $x): int {
    if ($x < 0) {
        return -$x;
    }
    return $x;
}

function my_max(int $a, int $b): int {
    if ($a > $b) {
        return $a;
    }
    return $b;
}

function my_min(int $a, int $b): int {
    if ($a < $b) {
        return $a;
    }
    return $b;
}

function clamp(int $val, int $lo, int $hi): int {
    return my_max($lo, my_min($val, $hi));
}

function gcd(int $a, int $b): int {
    $a = my_abs($a);
    $b = my_abs($b);
    while ($b != 0) {
        $t = $b;
        $b = $a % $b;
        $a = $t;
    }
    return $a;
}

function power(int $base, int $exp): int {
    $result = 1;
    for ($i = 0; $i < $exp; $i++) {
        $result *= $base;
    }
    return $result;
}

function describe(int|string $value): string {
    return gettype($value) . ":" . $value;
}

function describe_maybe(?int $value): string {
    if (is_null($value)) {
        return "NULL:null";
    }
    return gettype($value) . ":" . $value;
}

function add_ten(int $value = 10): int {
    return $value + 10;
}

function profile(string $name, int $age = 18): string {
    return $name . ":" . $age;
}

echo "my_abs(-42) = " . my_abs(-42) . "\n";
echo "my_max(3, 7) = " . my_max(3, 7) . "\n";
echo "clamp(15, 0, 10) = " . clamp(15, 0, 10) . "\n";
echo "gcd(48, 18) = " . gcd(48, 18) . "\n";
echo "2^10 = " . power(2, 10) . "\n";
echo "describe(42) = " . describe(42) . "\n";
echo "describe(null) = " . describe_maybe(null) . "\n";
echo "add_ten() = " . add_ten() . "\n";
echo "profile(age: 30, name: \"Ada\") = " . profile(age: 30, name: "Ada") . "\n";
