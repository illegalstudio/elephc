<?php

class RegexTagger {
    public function __construct(private string $prefix) {}

    public function tag(array $matches): string {
        if (count($matches) > 0) {
            return $this->prefix;
        }
        return "";
    }
}

function print_regex_tag(callable $callback): void {
    $descriptorTagged = preg_replace_callback("/[A-Z]/", $callback, "AB");
    echo "Descriptor callback replace: " . $descriptorTagged . "\n";
}

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

$unicode = "日本語123";
if (preg_match("/\p{L}+/u", $unicode)) {
    echo "Unicode letters detected\n";
}

// Replace
$cleaned = preg_replace("/[ ]+/", " ", "hello    world    test");
echo "Cleaned: " . $cleaned . "\n";

$numberGroups = preg_replace("/\p{N}+/u", "X", "abc123def456");
echo "Unicode property replace: " . $numberGroups . "\n";

$name = preg_replace("/([a-z]+) ([a-z]+)/", '$2, $1', "ada lovelace");
echo "Name swap: " . $name . "\n";

$tagged = preg_replace_callback("/(\d+)/", function($matches) {
    return "[" . $matches[0] . "]";
}, "order 42, item 7");
echo "Callback replace: " . $tagged . "\n";

print_regex_tag((new RegexTagger("descriptor:"))->tag(...));

// Split
$parts = preg_split("/[,;]+/", "one,two;;three,four");
echo "Parts: " . count($parts) . "\n";
$i = 0;
while ($i < count($parts)) {
    echo "  " . $parts[$i] . "\n";
    $i = $i + 1;
}
