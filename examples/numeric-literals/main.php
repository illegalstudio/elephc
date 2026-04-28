<?php

// Numeric literal forms supported by elephc, all 100% PHP-compatible.
// Underscores between digits are visual separators (PHP 7.4+) and are stripped
// before parsing — they only exist to make long numbers readable.

// --- Integer bases --------------------------------------------------------
// All four forms below produce the same value (255).
$decimal = 255;
$hex = 0xFF;
$octal = 0o377;       // PHP 8.1+ explicit octal
$binary = 0b1111_1111; // PHP 5.4+ binary, with separator

echo "decimal = " . $decimal . "\n";
echo "hex     = " . $hex . "\n";
echo "octal   = " . $octal . "\n";
echo "binary  = " . $binary . "\n";

// --- Numeric separators on real-world values ------------------------------
// File permissions: octal is the natural fit; the separator is uncommon here
// but legal — useful when you want to group user/group/other bits visually.
$chmod_mode = 0o7_5_5;
echo "chmod mode 0o7_5_5 = " . $chmod_mode . "\n"; // 493

// Big numbers stay readable when grouped in thousands.
$population = 7_900_000_000;
echo "world population ~ " . $population . "\n";

// Hex MAC-like value — group by byte.
$mac_low = 0xDE_AD_BE_EF;
echo "mac low 0xDE_AD_BE_EF = " . $mac_low . "\n";

// --- Floats with separators -----------------------------------------------
// Avogadro's number, with separators in the mantissa AND the exponent.
$avogadro = 6.022_140e2_3;
echo "Avogadro = " . $avogadro . "\n";

// Speed of light in m/s (exact, by definition).
$c = 299_792_458;
echo "c = " . $c . " m/s\n";
