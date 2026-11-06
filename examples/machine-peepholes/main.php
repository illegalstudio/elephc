<?php
// Late machine peepholes in action. Compile with `--emit-asm` and compare the functions:
// - `rolling_hash()`: the `for` test branches on the compare flags instead of materializing
//   a boolean, and the overflow path of the checked `* 31 + $i` chain lives out of line,
//   after the function's last block, so the loop body falls straight through;
// - `scaled()`: the masked operand lets range analysis prove each product fits, so the
//   multiplies are plain and strength-reduced (`* 31` to `(x << 5) - x`, `* 10` to a
//   `lea`/shifted add plus shift, `* -8` to shift and negate). `* 11` keeps the multiplier.

function rolling_hash(int $rounds): int
{
    $acc = 0;
    for ($i = 0; $i < $rounds; $i++) {
        $acc = ($acc * 31 + $i) & 0xFFFFFF;
    }
    return $acc;
}

function scaled(int $value): string
{
    $x = $value & 0xFFFF;
    return ($x * 31) . " " . ($x * 10) . " " . ($x * -8) . " " . ($x * 11);
}

echo "hash=", rolling_hash(1000 * $argc), "\n";
echo "scaled=", scaled(1234 * $argc), "\n";
