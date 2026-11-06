<?php
// A dynamic fragment shares the activation's scope across repeated eval calls.
function dynamicTotal(string $fragment, int $iterations): int
{
    $sum = 0;
    for ($i = 0; $i < $iterations; $i++) {
        eval($fragment);
    }
    return (int) $sum;
}

echo dynamicTotal('$sum += 3;', 10), "\n";

// Each call starts a fresh eval scope and reuses the native module catalog.
echo dynamicTotal('$sum += 3;', 2), "\n";
echo dynamicTotal('$sum += 3;', 3), "\n";
