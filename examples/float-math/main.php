<?php
// Float math operations

$pi = 3.14159265358979;
$e = 2.71828182845905;

echo "Pi: " . $pi . "\n";
echo "e:  " . $e . "\n";

// Basic arithmetic
echo "\nArithmetic:\n";
echo "  pi + e  = " . ($pi + $e) . "\n";
echo "  pi * 2  = " . ($pi * 2) . "\n";
echo "  10 / 3  = " . (10.0 / 3.0) . "\n";

// Math functions
echo "\nMath functions:\n";
echo "  floor(3.7)  = " . floor(3.7) . "\n";
echo "  ceil(3.2)   = " . ceil(3.2) . "\n";
echo "  round(3.5)  = " . round(3.5) . "\n";
echo "  sqrt(2)     = " . sqrt(2.0) . "\n";
echo "  pow(2, 10)  = " . pow(2.0, 10.0) . "\n";
echo "  abs(-42)    = " . abs(-42) . "\n";
echo "  abs(-3.14)  = " . abs(-3.14) . "\n";

// Min/max
echo "\nMin/Max:\n";
echo "  min(3, 7)     = " . min(3, 7) . "\n";
echo "  max(1.5, 2.5) = " . max(1.5, 2.5) . "\n";

// Integer division
echo "\nInteger division:\n";
echo "  intdiv(7, 2)  = " . intdiv(7, 2) . "\n";

// Type checks
echo "\nType checks:\n";
echo "  is_float(3.14) = " . is_float(3.14) . "\n";
echo "  is_int(42)     = " . is_int(42) . "\n";
echo "  is_string(\"x\") = " . is_string("x") . "\n";
