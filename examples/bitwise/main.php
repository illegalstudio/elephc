<?php

// Bitwise operators, spaceship, and null coalescing

// Bitwise AND, OR, XOR
$flags = 5;  // binary: 101
echo "flags & 3 = " . ($flags & 3) . "\n";   // 1 (binary: 001)
echo "flags | 2 = " . ($flags | 2) . "\n";   // 7 (binary: 111)
echo "flags ^ 3 = " . ($flags ^ 3) . "\n";   // 6 (binary: 110)

// Bitwise NOT
echo "~0 = " . ~0 . "\n";                     // -1

// Shift operators
echo "1 << 8 = " . (1 << 8) . "\n";           // 256
echo "256 >> 4 = " . (256 >> 4) . "\n";       // 16

// Compound assignment operators
$mask = 15;
$mask &= 10;
echo "mask after &= 10 = " . $mask . "\n";     // 10
$mask |= 1;
echo "mask after |= 1 = " . $mask . "\n";      // 11
$mask ^= 3;
echo "mask after ^= 3 = " . $mask . "\n";      // 8
$mask <<= 2;
echo "mask after <<= 2 = " . $mask . "\n";     // 32
$mask >>= 4;
echo "mask after >>= 4 = " . $mask . "\n";     // 2

$pow = 2;
$pow **= 5;
echo "pow after **= 5 = " . $pow . "\n";       // 32

// Spaceship operator <=>
echo "1 <=> 2 = " . (1 <=> 2) . "\n";         // -1
echo "2 <=> 2 = " . (2 <=> 2) . "\n";         //  0
echo "3 <=> 2 = " . (3 <=> 2) . "\n";         //  1

// Null coalescing ??
$x = null;
$y = 42;
echo "null ?? 'default' = " . ($x ?? "default") . "\n";
echo "42 ?? 'default' = " . ($y ?? "default") . "\n";
echo "null ?? null ?? 'found' = " . ($x ?? $x ?? "found") . "\n";

// Null coalescing assignment ??=
$name = null;
$name ??= "guest";
$name ??= "ignored";
echo "name after ??= = " . $name . "\n";
