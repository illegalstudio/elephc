<?php

// Binary integer literals (PHP 5.4+) — write a bit pattern directly with `0b`.
// Use cases: feature flags, hardware registers, color packing, network masks.

// --- Feature flags ---
// Each flag is a single bit — combine with `|`, test with `&`.
$READABLE = 0b0001;
$WRITABLE = 0b0010;
$EXECUTABLE = 0b0100;
$HIDDEN = 0b1000;

$perms = $READABLE | $WRITABLE;
echo "rw flags (binary 0b0011) = " . $perms . "\n";

if (($perms & $WRITABLE) !== 0) {
    echo "  -> writable bit is set\n";
}
if (($perms & $EXECUTABLE) === 0) {
    echo "  -> executable bit is NOT set\n";
}

// --- RGB packing into a single 24-bit integer ---
// Each channel takes 8 bits. Underscores (PHP 7.4+) make the layout obvious.
$red = 0b1111_1111;   // 255
$green = 0b1000_0000; // 128
$blue = 0b0000_0001;  //   1

$rgb = ($red << 16) | ($green << 8) | $blue;
echo "rgb packed = " . $rgb . "\n"; // 16744449

// Recover one channel back from the packed value.
$recovered_green = ($rgb >> 8) & 0xFF;
echo "green channel back = " . $recovered_green . "\n"; // 128

// --- Net mask --------------------------------------------------------------
// `0b...` makes the mask self-documenting compared to a decimal literal.
$ipv4_mask_24 = 0b11111111_11111111_11111111_00000000;
echo "/24 mask = " . $ipv4_mask_24 . "\n"; // 4294967040
