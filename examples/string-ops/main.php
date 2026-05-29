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
echo "index[1]: " . $str[1] . "\n";
echo "index[-1]: " . $str[-1] . "\n";
echo "index[99]: [" . $str[99] . "]\n";

// Case
echo "\n--- Case ---\n";
echo "strtolower: " . strtolower($str) . "\n";
echo "strtoupper: " . strtoupper($str) . "\n";
echo "ucfirst: " . ucfirst("hello") . "\n";
echo "lcfirst: " . lcfirst("HELLO") . "\n";

// Trimming
echo "\n--- Trim ---\n";
echo "trim: [" . trim("  spaced  ") . "]\n";
echo "trim form-feed: [" . trim("\f boxed \f") . "]\n";
echo "ltrim form-feed: [" . ltrim("\fleft") . "]\n";
echo "rtrim form-feed: [" . rtrim("right\f") . "]\n";
echo "chop form-feed: [" . chop("tail\f") . "]\n";

// Transform
echo "\n--- Transform ---\n";
echo "str_repeat: " . str_repeat("ha", 3) . "\n";
echo "strrev: " . strrev("desserts") . "\n";
echo "grapheme_strrev: " . grapheme_strrev("A\u{0065}\u{0301}\u{1F469}\u{1F3FD}\u{200D}\u{1F4BB}") . "\n";
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

// String interpolation
echo "\n--- Interpolation ---\n";
$name = "PHP";
echo "Hello $name!\n";

// Escape sequences
echo "\n--- Escapes ---\n";
$binary = "A\x00B";
echo "hex/octal/unicode: " . "\x41\101\u{1F600}" . "\n";
echo "null byte length: " . strlen($binary) . ", ord: " . ord($binary[1]) . "\n";

// Formatting
echo "\n--- Formatting ---\n";
echo sprintf("Name: %s, Age: %d", "Alice", 30) . "\n";
echo sprintf("Hex: %x", 255) . "\n";

// Hashing
echo "\n--- Hashing ---\n";
echo "md5('hello'): " . md5("hello") . "\n";
echo "sha1('hello'): " . sha1("hello") . "\n";
echo "hash('sha1', 'hello'): " . hash("sha1", "hello") . "\n";

// Encoding
echo "\n--- Encoding ---\n";
echo "htmlspecialchars: " . htmlspecialchars("<b>bold</b>") . "\n";
echo "urlencode: " . urlencode("hello world") . "\n";
echo "base64: " . base64_encode("Hello") . "\n";

// Validation
echo "\n--- Validation ---\n";
echo "ctype_alpha('abc'): " . (ctype_alpha("abc") ? "yes" : "no") . "\n";
echo "ctype_digit('123'): " . (ctype_digit("123") ? "yes" : "no") . "\n";

// Parsing
echo "\n--- Parsing ---\n";
$parsed = sscanf("X=42 Y=99", "X=%d Y=%d");
echo "sscanf count: " . count($parsed) . "\n";
echo "sscanf values: " . $parsed[0] . ", " . $parsed[1] . "\n";
