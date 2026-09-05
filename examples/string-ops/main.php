<?php
// String operations

$str = "Hello, World!";

// Searching
echo "--- Search ---\n";
echo "strpos: " . strpos($str, "World") . "\n";
// stripos()/strripos() are the case-insensitive twins of strpos()/strrpos();
// the optional $offset works the same way, negative values included
echo "stripos: " . stripos($str, "WORLD") . "\n";
echo "strripos: " . strripos($str, "O") . "\n";
echo "stripos(offset): " . stripos($str, "L", 4) . "\n";
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
echo "mb_strtolower: " . mb_strtolower("HÉLLO") . "\n";
echo "mb_strtolower 8bit: " . bin2hex(mb_strtolower("HÉLLO", "8bit")) . "\n";
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

// Wrapping (word-aware; cut_long_words breaks over-long words)
echo "\n--- Wrap ---\n";
echo "wordwrap(15):\n" . wordwrap("The quick brown fox jumped", 15) . "\n";
echo "wordwrap(8, cut):\n" . wordwrap("A verylongword", 8, "\n", true) . "\n";

// Chunking and escaping
echo "\n--- Chunk/Escape ---\n";
// chunk_split() appends the separator after every chunk, including the trailing partial one
echo "chunk_split(3): " . chunk_split("abcdefgh", 3, "-") . "\n";
// quotemeta() backslash-escapes the regular-expression metacharacters
echo "quotemeta: " . quotemeta('cost: $5 (approx.) [net]') . "\n";
// A $length below 1 is a catchable \ValueError
try {
    chunk_split("abc", 0);
} catch (\ValueError $e) {
    echo "caught: " . $e->getMessage() . "\n";
}

// Word and byte statistics
echo "\n--- Stats ---\n";
$sentence = "Hello friend, you're looking good today!";
// Format 0 counts words, 1 returns the list, 2 keys every word by its byte offset
echo "str_word_count: " . str_word_count($sentence) . "\n";
echo "str_word_count(1): " . implode(", ", str_word_count($sentence, 1)) . "\n";
foreach (str_word_count("one two", 2) as $offset => $word) {
    echo "  offset $offset => $word\n";
}
// $characters widens the word alphabet beyond letters, ' and -
echo "str_word_count digits: " . implode(", ", str_word_count("fri3nd", 1, "3")) . "\n";
// count_chars() mode 1 tallies only the byte values the string actually uses
foreach (count_chars("hello", 1) as $byte => $count) {
    echo "  " . chr($byte) . " x $count\n";
}
// modes 3 and 4 render the used / unused byte values as a string
echo "count_chars(3): " . count_chars("hello world", 3) . "\n";
// A mode outside 0..4 is a catchable \ValueError
try {
    count_chars("abc", 9);
} catch (\ValueError $e) {
    echo "caught: " . $e->getMessage() . "\n";
}

// Translation
echo "\n--- Translate ---\n";
// Three arguments translate bytes pairwise, truncated to the shorter list
echo "strtr(pairwise): " . strtr("abcd", "abc", "xy") . "\n";
// Two arguments apply replacement pairs longest-match-first, in one left-to-right pass
echo "strtr(pairs): " . strtr("foo bar", ["foo" => "bar", "bar" => "baz"]) . "\n";
echo "strtr(longest): " . strtr("abc", ["a" => "b", "ab" => "X"]) . "\n";

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

// Hashing — hash() exposes the full elephc-crypto algorithm set
echo "\n--- Hashing ---\n";
echo "md5('hello'): " . md5("hello") . "\n";
echo "sha1('hello'): " . sha1("hello") . "\n";
echo "hash('sha1', 'hello'): " . hash("sha1", "hello") . "\n";
echo "hash('sha256', 'hello'): " . hash("sha256", "hello") . "\n";
echo "hash('sha512', 'hello'): " . hash("sha512", "hello") . "\n";
echo "hash('sha3-256', 'hello'): " . hash("sha3-256", "hello") . "\n";
echo "hash('crc32b', 'hello'): " . hash("crc32b", "hello") . "\n";
// $binary=true returns the raw digest bytes; bin2hex renders them readable
echo "raw sha256 length: " . strlen(hash("sha256", "hello", true)) . "\n";
echo "raw sha256 hex: " . bin2hex(hash("sha256", "hello", true)) . "\n";
// hash_hmac() computes a keyed message authentication code
echo "hmac sha256: " . hash_hmac("sha256", "what do ya want for nothing?", "Jefe") . "\n";
echo "hmac sha1: " . hash_hmac("sha1", "hello", "key") . "\n";
// An unknown algorithm throws a catchable \ValueError
try {
    hash("definitely-not-an-algo", "hello");
} catch (\ValueError $e) {
    echo "caught: " . $e->getMessage() . "\n";
}
// hash_hmac() additionally rejects non-cryptographic checksums with \ValueError
try {
    hash_hmac("crc32b", "hello", "key");
} catch (\ValueError $e) {
    echo "caught: " . $e->getMessage() . "\n";
}

// Encoding
echo "\n--- Encoding ---\n";
echo "mb_strlen UTF-8: " . mb_strlen("héllo", "UTF-8") . "\n";
echo "mb_strlen bytes: " . mb_strlen("héllo", "8bit") . "\n";
echo "htmlspecialchars: " . htmlspecialchars("<b>bold</b>") . "\n";
echo "urlencode: " . urlencode("hello world") . "\n";
echo "base64: " . base64_encode("Hello") . "\n";
// base64_decode() skips whitespace and tolerates missing padding; $strict = true
// instead returns false for anything outside the Base64 alphabet
echo "base64_decode: " . base64_decode("SGVs bG8") . "\n";
var_dump(base64_decode("SGVsbG8*", true));
// quoted_printable_encode() escapes control, high-bit, and "=" bytes as =XX
echo "quoted_printable_encode: " . quoted_printable_encode("caf\xC3\xA9 = 1\tunit") . "\n";

// Validation
echo "\n--- Validation ---\n";
echo "ctype_alpha('abc'): " . (ctype_alpha("abc") ? "yes" : "no") . "\n";
echo "ctype_digit('123'): " . (ctype_digit("123") ? "yes" : "no") . "\n";

// Parsing
echo "\n--- Parsing ---\n";
$parsed = sscanf("X=42 Y=99", "X=%d Y=%d");
echo "sscanf count: " . count($parsed) . "\n";
echo "sscanf values: " . $parsed[0] . ", " . $parsed[1] . "\n";

// Increment (PHP's perl-style alphanumeric carry)
echo "\n--- Increment ---\n";
$col = "A";
$cols = [];
for ($i = 0; $i < 28; $i++) { $cols[] = $col; $col++; }
echo "columns: " . implode(" ", $cols) . "\n";
$word = "az";
$word++;
echo "'az'++ : " . $word . "\n";
$wrap = "Zz";
$wrap++;
echo "'Zz'++ : " . $wrap . "\n";
$num = "9";
$num++;
echo "'9'++  : ";
var_dump($num);
