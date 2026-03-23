<?php
// Associative arrays

$user = ["name" => "Alice", "city" => "NYC", "lang" => "PHP"];

echo "Name: " . $user["name"] . "\n";
echo "City: " . $user["city"] . "\n";

// Update a value
$user["city"] = "SF";
echo "Moved to: " . $user["city"] . "\n";

// Add a new key
$user["age"] = "30";
echo "Age: " . $user["age"] . "\n";

// Iterate with key => value
echo "\nAll fields:\n";
foreach ($user as $key => $value) {
    echo "  " . $key . " = " . $value . "\n";
}

// Integer-valued associative array
$scores = ["math" => 95, "english" => 87, "science" => 92];
$total = $scores["math"] + $scores["english"] + $scores["science"];
echo "\nTotal score: " . $total . "\n";
