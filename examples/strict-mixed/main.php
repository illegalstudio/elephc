<?php
function addBonus(mixed $points): mixed {
    if (is_int($points)) {
        return ($points + 1) + 2;
    }
    return 0;
}

function addNumeric(int|float $points): mixed {
    return $points + 1;
}

echo addBonus(3), ':', addBonus('missing'), ':', addNumeric(2.5);
