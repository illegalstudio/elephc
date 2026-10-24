<?php
// Two passes over the same samples reuse one counter name. Each loop keeps its own
// binding of $i: the forward pass counts with a raw integer, although the backward pass
// starts from $n - 1, a checked subtraction that could overflow to a float in PHP.
function smooth(array $samples): array {
    $n = count($samples);
    $forward = [];
    $carry = 0;
    for ($i = 0; $i < $n; $i++) {
        $carry = intdiv($carry + $samples[$i], 2);
        $forward[] = $carry;
    }
    $backward = array_fill(0, $n, 0);
    $carry = 0;
    for ($i = $n - 1; $i >= 0; $i--) {
        $carry = intdiv($carry + $forward[$i], 2);
        $backward[$i] = $carry;
    }
    return $backward;
}

// Prints: 6 9 8 9 10 6 5 4
echo implode(' ', smooth([8, 16, 4, 12, 20, 0, 6, 10])), "\n";
