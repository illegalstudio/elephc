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
echo "  10 / 3  = " . (10 / 3) . "\n";

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

// Exponentiation
echo "\nExponentiation:\n";
echo "  2 ** 10     = " . (2 ** 10) . "\n";
echo "  2 ** 0.5    = " . (2 ** 0.5) . "\n";

// fmod, fdiv
echo "\nfmod/fdiv:\n";
echo "  fmod(10.5, 3.2) = " . fmod(10.5, 3.2) . "\n";
echo "  fdiv(10, 3)     = " . fdiv(10, 3) . "\n";

// Integer division
echo "\nInteger division:\n";
echo "  intdiv(7, 2)  = " . intdiv(7, 2) . "\n";

// number_format
echo "\nnumber_format:\n";
echo "  number_format(1234567)    = " . number_format(1234567) . "\n";
echo "  number_format(1234.56, 2) = " . number_format(1234.56, 2) . "\n";

// Constants
echo "\nConstants:\n";
echo "  PHP_INT_MAX = " . PHP_INT_MAX . "\n";
echo "  M_PI        = " . M_PI . "\n";

// Special values
echo "\nSpecial values:\n";
echo "  INF       = " . INF . "\n";
echo "  -INF      = " . -INF . "\n";
echo "  NAN       = " . NAN . "\n";
// Type checks
echo "\nType checks:\n";
echo "  is_float(3.14)    = " . is_float(3.14) . "\n";
echo "  is_int(42)        = " . is_int(42) . "\n";
echo "  is_string(\"x\")    = " . is_string("x") . "\n";
echo "  is_nan(NAN)       = " . is_nan(NAN) . "\n";
echo "  is_infinite(INF)  = " . is_infinite(INF) . "\n";
echo "  is_finite(3.14)   = " . is_finite(3.14) . "\n";
echo "  is_finite(INF)    = " . is_finite(INF) . "\n";
