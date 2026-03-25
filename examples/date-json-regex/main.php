<?php

// Date/time formatting
$ts = mktime(14, 30, 0, 12, 25, 2024);
echo "Christmas 2024: " . date("l, F j, Y", $ts) . "\n";
echo "Time: " . date("g:i A", $ts) . "\n";
echo "ISO: " . date("Y-m-d H:i:s", $ts) . "\n";

// Parse date strings back to timestamps
$parsed = strtotime("2024-12-25 14:30:00");
echo "Parsed timestamp matches: ";
if ($parsed == $ts) {
    echo "yes\n";
} else {
    echo "no\n";
}

// JSON encoding
echo "\n--- JSON Encoding ---\n";
echo "Integer: " . json_encode(42) . "\n";
echo "Float:   " . json_encode(3.14) . "\n";
echo "String:  " . json_encode("hello world") . "\n";
echo "Bool:    " . json_encode(true) . "\n";
echo "Null:    " . json_encode(null) . "\n";
echo "Array:   " . json_encode([1, 2, 3]) . "\n";

$config = ["host" => "localhost", "port" => "8080"];
echo "Object:  " . json_encode($config) . "\n";

// JSON decoding
$decoded = json_decode("\"hello world\"");
echo "Decoded: " . $decoded . "\n";
echo "Errors:  " . json_last_error() . "\n";

// Regular expressions
echo "\n--- Regex ---\n";

// Pattern matching
$email = "user@example.com";
if (preg_match("/[a-z]+@[a-z]+\\.[a-z]+/", $email)) {
    echo "Valid email pattern\n";
}

// Count matches
$text = "The quick brown fox jumps over the lazy dog";
$words = preg_match_all("/[a-z]+/", $text);
echo "Word count: " . $words . "\n";

// Replace
$cleaned = preg_replace("/[ ]+/", " ", "hello    world    test");
echo "Cleaned: " . $cleaned . "\n";

// Split
$parts = preg_split("/[,;]+/", "one,two;;three,four");
echo "Parts: " . count($parts) . "\n";
$i = 0;
while ($i < count($parts)) {
    echo "  " . $parts[$i] . "\n";
    $i = $i + 1;
}
