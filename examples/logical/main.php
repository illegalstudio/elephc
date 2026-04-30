<?php
// Logical operators and boolean literals

$age = 25;
$has_license = 1;

$can_drive = $age >= 18 && $has_license;
echo "Can drive: " . ($can_drive ? "yes" : "no") . "\n";

$is_minor = $age < 18;
echo "Is minor: " . ($is_minor ? "yes" : "no") . "\n";

// Short-circuit evaluation
$x = 0;
$result = $x != 0 && 100 / $x > 5;
echo "Safe division (short-circuited): " . ($result ? "yes" : "no") . "\n";

// Combining conditions
$temp = 22;
$is_comfortable = $temp >= 20 && $temp <= 26;
echo "Temperature " . $temp . " is comfortable: " . ($is_comfortable ? "yes" : "no") . "\n";

// NOT operator
$is_weekend = 0;
$should_work = !$is_weekend;
echo "Should work: " . ($should_work ? "yes" : "no") . "\n";

// OR with fallback
$primary = 0;
$backup = 42;
$value = $primary || $backup;
echo "Value (0 || 42): " . ($value ? "yes" : "no") . "\n";

// Word-form logical operators use PHP's lower precedence
$word_and = (true || false and false);
echo "Word and precedence: " . ($word_and ? "yes" : "no") . "\n";

$word_or = (false && true or true);
echo "Word or precedence: " . ($word_or ? "yes" : "no") . "\n";

$word_xor = (true xor true and false);
echo "Word xor precedence: " . ($word_xor ? "yes" : "no") . "\n";

// Assignment expressions bind tighter than word-form logical operators
$assigned = true and false;
echo "Assignment before word and: " . ($assigned ? "yes" : "no") . "\n";

$score = 10;
echo "Assignment expression value: " . ($score += 5) . "\n";

function bonus_index(): int {
    return 1;
}

$scores = [2, 4];
echo "Array assignment expression value: " . ($scores[bonus_index()] += 3) . "\n";

$slot = 0;
echo "RHS-mutated index assignment: " . ($scores[$slot] = ($slot = 1)) . "\n";
echo "Scores after stabilized writes: " . $scores[0] . ", " . $scores[1] . "\n";

// Short ternary / Elvis keeps the left value when truthy
$nickname = "";
$display_name = $nickname ?: "anonymous";
echo "Display name: " . $display_name . "\n";
