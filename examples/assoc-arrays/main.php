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

// Remove a key with unset(): count drops and isset() reports it gone
unset($user["age"]);
echo "After unset, has age: " . (isset($user["age"]) ? "yes" : "no") . "\n";
echo "Field count: " . count($user) . "\n";

// PHP key normalization: "1" and 1 are the same key, but "01" stays a string key
$codes = [1 => "one", "2" => "two", "01" => "leading"];
$codes["1"] = "ONE";
echo "Code 1: " . $codes[1] . "\n";
echo "Code 2: " . $codes["2"] . "\n";
echo "Code 01: " . $codes["01"] . "\n";
echo "Codes JSON: " . json_encode($codes) . "\n";

// Iterate with key => value
echo "\nAll fields:\n";
foreach ($user as $key => $value) {
    echo "  " . $key . " = " . $value . "\n";
}

// Integer-valued associative array
$scores = ["math" => 95, "english" => 87, "science" => 92];
$total = $scores["math"] + $scores["english"] + $scores["science"];
echo "\nTotal score: " . $total . "\n";

// array_key_exists
echo "\nKey checks:\n";
if (array_key_exists("math", $scores)) {
    echo "  math exists\n";
}
if (!array_key_exists("art", $scores)) {
    echo "  art does not exist\n";
}

// in_array — search by value
echo "\nValue search:\n";
if (in_array(95, $scores)) {
    echo "  someone scored 95\n";
}
if (!in_array(100, $scores)) {
    echo "  nobody scored 100\n";
}

// array_search — find key by value
$subject = array_search("Alice", $user);
echo "\nAlice found at key: " . $subject . "\n";

// array_keys — get all keys
echo "\nScore subjects: ";
$keys = array_keys($scores);
$n = count($keys);
for ($i = 0; $i < $n; $i++) {
    echo $keys[$i];
    if ($i < $n - 1) {
        echo ", ";
    }
}
echo "\n";

// array_values — get all values
echo "Score values: ";
$vals = array_values($scores);
$n = count($vals);
for ($i = 0; $i < $n; $i++) {
    echo $vals[$i];
    if ($i < $n - 1) {
        echo ", ";
    }
}
echo "\n";

// PHP array union: duplicate keys keep the left value
$defaults = ["theme" => "light", "lang" => "en"];
$overrides = ["lang" => "it", "timezone" => "Europe/Rome"];
$settings = $defaults + $overrides;
echo "\nSettings union:\n";
foreach ($settings as $key => $value) {
    echo "  " . $key . " = " . $value . "\n";
}

// Indexed and associative operands share the same normalized key space
$base = ["slot 0", "slot 1"];
$labels = ["1" => "ignored duplicate", "01" => "string key", "name" => "display"];
$mixedUnion = $base + $labels;
echo "\nMixed representation union:\n";
foreach ($mixedUnion as $key => $value) {
    echo "  " . $key . " = " . $value . "\n";
}

// Mixed-value associative arrays keep a runtime tag per entry
$profile = ["name" => "Alice", "age" => 30, "active" => true, "note" => null];
echo "\nMixed profile:\n";
foreach ($profile as $key => $value) {
    echo "  " . $key . " = ";
    echo $value;
    echo "\n";
}
echo "As JSON: " . json_encode($profile) . "\n";
