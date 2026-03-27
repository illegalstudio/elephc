<?php
// Mathematical functions showcase

echo "=== Trigonometry ===\n";
echo "sin(π/6) = " . round(sin(M_PI / 6), 4) . "\n";
echo "cos(π/3) = " . round(cos(M_PI / 3), 4) . "\n";
echo "tan(π/4) = " . round(tan(M_PI_4), 4) . "\n";

echo "\n=== Inverse Trig ===\n";
echo "asin(0.5) = " . round(asin(0.5), 4) . " rad (" . round(rad2deg(asin(0.5)), 1) . "°)\n";
echo "atan2(1, 1) = " . round(rad2deg(atan2(1.0, 1.0)), 1) . "°\n";

echo "\n=== Logarithms ===\n";
echo "ln(e) = " . log(M_E) . "\n";
echo "log2(256) = " . log2(256.0) . "\n";
echo "log10(10000) = " . log10(10000.0) . "\n";

echo "\n=== Distance ===\n";
$dist = hypot(3.0, 4.0);
echo "distance(0,0 → 3,4) = " . $dist . "\n";

echo "\n=== Constants ===\n";
echo "π = " . M_PI . "\n";
echo "e = " . M_E . "\n";
echo "√2 = " . M_SQRT2 . "\n";
