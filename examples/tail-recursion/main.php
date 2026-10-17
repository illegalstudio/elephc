<?php
// Euclid's algorithm uses a tail call to continue with the next pair.
function gcd(int $a, int $b): int {
    if ($b === 0) {
        return $a;
    }
    return gcd($b, $a % $b);
}

// Mutually recursive states recognize whether a countdown has even length.
function evenSteps(int $remaining): bool {
    if ($remaining === 0) {
        return true;
    }
    return oddSteps($remaining - 1);
}

function oddSteps(int $remaining): bool {
    if ($remaining === 0) {
        return false;
    }
    return evenSteps($remaining - 1);
}

echo "gcd(1071, 462) = ", gcd(1071, 462), "\n";
echo "300000 steps: ", evenSteps(300000) ? "even" : "odd", "\n";
