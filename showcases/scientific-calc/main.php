<?php
// Scientific Calculator — computed results showcase
// Demonstrates all math functions compiled to native code

echo "================================\n";
echo "  SCIENTIFIC CALCULATOR (elephc)\n";
echo "================================\n\n";

// --- Trigonometry ---
echo "--- Trigonometry ---\n";
echo "sin(0)     = " . sin(0.0) . "\n";
echo "sin(π/6)   = " . round(sin(M_PI / 6), 6) . "\n";
echo "sin(π/2)   = " . round(sin(M_PI_2), 6) . "\n";
echo "cos(0)     = " . cos(0.0) . "\n";
echo "cos(π/3)   = " . round(cos(M_PI / 3), 6) . "\n";
echo "cos(π)     = " . round(cos(M_PI), 6) . "\n";
echo "tan(0)     = " . tan(0.0) . "\n";
echo "tan(π/4)   = " . round(tan(M_PI_4), 6) . "\n";

// --- Inverse Trigonometry ---
echo "\n--- Inverse Trigonometry ---\n";
echo "asin(0.5)  = " . round(asin(0.5), 6) . " rad = " . round(rad2deg(asin(0.5)), 2) . "°\n";
echo "asin(1)    = " . round(asin(1.0), 6) . " rad = " . round(rad2deg(asin(1.0)), 2) . "°\n";
echo "acos(0.5)  = " . round(acos(0.5), 6) . " rad = " . round(rad2deg(acos(0.5)), 2) . "°\n";
echo "atan(1)    = " . round(atan(1.0), 6) . " rad = " . round(rad2deg(atan(1.0)), 2) . "°\n";
echo "atan2(1,1) = " . round(atan2(1.0, 1.0), 6) . " rad = " . round(rad2deg(atan2(1.0, 1.0)), 2) . "°\n";

// --- Hyperbolic ---
echo "\n--- Hyperbolic ---\n";
echo "sinh(1)    = " . round(sinh(1.0), 6) . "\n";
echo "cosh(1)    = " . round(cosh(1.0), 6) . "\n";
echo "tanh(1)    = " . round(tanh(1.0), 6) . "\n";

// --- Logarithms & Exponentials ---
echo "\n--- Logarithms & Exponentials ---\n";
echo "log(e)     = " . log(M_E) . "\n";
echo "log(10)    = " . round(log(10.0), 6) . "\n";
echo "log2(256)  = " . log2(256.0) . "\n";
echo "log10(1M)  = " . log10(1000000.0) . "\n";
echo "exp(0)     = " . exp(0.0) . "\n";
echo "exp(1)     = " . round(exp(1.0), 6) . "\n";
echo "exp(2)     = " . round(exp(2.0), 6) . "\n";

// --- Utility Functions ---
echo "\n--- Utility ---\n";
echo "sqrt(144)  = " . sqrt(144.0) . "\n";
echo "pow(2, 10) = " . pow(2.0, 10.0) . "\n";
echo "hypot(3,4) = " . hypot(3.0, 4.0) . "\n";
echo "pi()       = " . pi() . "\n";
echo "deg2rad(90)  = " . round(deg2rad(90.0), 6) . "\n";
echo "rad2deg(π/2) = " . rad2deg(M_PI_2) . "\n";

// --- Constants ---
echo "\n--- Constants ---\n";
echo "M_PI       = " . M_PI . "\n";
echo "M_E        = " . M_E . "\n";
echo "M_SQRT2    = " . M_SQRT2 . "\n";
echo "M_PI_2     = " . M_PI_2 . "\n";
echo "M_PI_4     = " . M_PI_4 . "\n";
echo "M_LOG2E    = " . M_LOG2E . "\n";
echo "M_LOG10E   = " . M_LOG10E . "\n";

// --- Real-World: Triangle Solver ---
echo "\n--- Triangle Solver ---\n";
// Given a right triangle with legs 5 and 12
$a = 5.0;
$b = 12.0;
$c = hypot($a, $b);
$angle_a = rad2deg(atan2($a, $b));
$angle_b = rad2deg(atan2($b, $a));
echo "Right triangle (a=" . $a . ", b=" . $b . "):\n";
echo "  hypotenuse = " . $c . "\n";
echo "  angle A    = " . round($angle_a, 2) . "°\n";
echo "  angle B    = " . round($angle_b, 2) . "°\n";
echo "  angle C    = 90°\n";
echo "  verify: A+B+C = " . round($angle_a + $angle_b + 90.0, 2) . "°\n";

// --- Real-World: Compound Interest ---
echo "\n--- Compound Interest ---\n";
$principal = 1000.0;
$rate = 0.05;
$years = 10.0;
$compound = $principal * exp($rate * $years);
echo "Principal: $" . $principal . "\n";
echo "Rate: " . round($rate * 100.0, 1) . "% continuously compounded\n";
echo "After " . $years . " years: $" . round($compound, 2) . "\n";
