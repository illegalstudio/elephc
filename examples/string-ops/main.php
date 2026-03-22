<?php
// String operations

$str = "Hello, World!";

// Searching
echo "--- Search ---\n";
echo "strpos: " . strpos($str, "World") . "\n";
echo "str_contains: " . (str_contains($str, "World") ? "yes" : "no") . "\n";
echo "str_starts_with: " . (str_starts_with($str, "Hello") ? "yes" : "no") . "\n";
echo "str_ends_with: " . (str_ends_with($str, "!") ? "yes" : "no") . "\n";

// Extracting
echo "\n--- Extract ---\n";
echo "substr(7): " . substr($str, 7) . "\n";
echo "substr(0, 5): " . substr($str, 0, 5) . "\n";
echo "strstr(@): " . strstr("user@example.com", "@") . "\n";

// Case
echo "\n--- Case ---\n";
echo "strtolower: " . strtolower($str) . "\n";
echo "strtoupper: " . strtoupper($str) . "\n";
echo "ucfirst: " . ucfirst("hello") . "\n";
echo "lcfirst: " . lcfirst("HELLO") . "\n";

// Trimming
echo "\n--- Trim ---\n";
echo "trim: [" . trim("  spaced  ") . "]\n";

// Transform
echo "\n--- Transform ---\n";
echo "str_repeat: " . str_repeat("ha", 3) . "\n";
echo "strrev: " . strrev("desserts") . "\n";
echo "str_replace: " . str_replace("World", "PHP", $str) . "\n";

// Split and join
echo "\n--- Split/Join ---\n";
$csv = "one,two,three";
$parts = explode(",", $csv);
echo "explode: " . count($parts) . " parts\n";
echo "implode: " . implode(" | ", $parts) . "\n";

// Character functions
echo "\n--- Char ---\n";
echo "ord('A'): " . ord("A") . "\n";
echo "chr(65): " . chr(65) . "\n";
