<?php
// Logical operators and boolean literals

$age = 25;
$has_license = true;

$can_drive = $age >= 18 && $has_license;
echo "Can drive: " . $can_drive . "\n";

$is_minor = $age < 18;
echo "Is minor: " . $is_minor . "\n";

// Short-circuit evaluation
$x = 0;
$result = $x != 0 && 100 / $x > 5;
echo "Safe division (short-circuited): " . $result . "\n";

// Combining conditions
$temp = 22;
$is_comfortable = $temp >= 20 && $temp <= 26;
echo "Temperature " . $temp . " is comfortable: " . $is_comfortable . "\n";

// NOT operator
$is_weekend = false;
$should_work = !$is_weekend;
echo "Should work: " . $should_work . "\n";

// OR with fallback
$primary = 0;
$backup = 42;
$value = $primary || $backup;
echo "Value (0 || 42): " . $value . "\n";
