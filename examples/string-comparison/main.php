<?php
// Relational comparison between two strings (`<`, `<=`, `>`, `>=`).
//
// PHP does NOT order two strings by bytes. When both sides are numeric strings it compares
// them numerically; otherwise it compares byte-wise. That is a different rule from strcmp(),
// which is always lexicographic — and the difference is visible in the very first line below.

// Both numeric: 10 is greater than 9, even though the byte "1" sorts before "9".
var_dump("10" > "9");

// "9a" is not a numeric string, so this pair falls back to byte order: "1" < "9".
var_dump("10" < "9a");

// Exponent and trailing-zero notations are numeric, so these compare equal.
var_dump("1e2" == "100");
var_dump("1.5" <= "1.50");

// Hexadecimal strings are NOT numeric in PHP, so bytes decide: "0" < "2".
var_dump("0x1A" < "26");

// The character-range idiom this exists for: classifying a character without ord().
function classify(string $c): string
{
    if ($c >= '0' && $c <= '9') {
        return "digit";
    }
    if (($c >= 'a' && $c <= 'z') || ($c >= 'A' && $c <= 'Z')) {
        return "letter";
    }
    return "other";
}

foreach (["5", "q", "Q", "!"] as $c) {
    echo $c, " => ", classify($c), "\n";
}

// Ordinary lexicographic ordering still works for non-numeric strings.
$words = ["pear", "apple", "fig"];
$smallest = $words[0];
foreach ($words as $word) {
    if ($word < $smallest) {
        $smallest = $word;
    }
}
echo "smallest: ", $smallest, "\n";

// Two INTEGER strings compare exactly, as int — they are not rounded through a float first.
// These two differ only past 2^53, which is precisely where a float can no longer tell them
// apart, so comparing them as floats would call them equal.
$hi = "9007199254740993";
$lo = "9007199254740992";
var_dump($hi > $lo);
var_dump($hi == $lo);

// Integer text too large for int follows PHP's own fallback rather than the float value:
// two sides that overflowed the same way compare by BYTES. That is why this is true even
// though it is numerically false — "9..." sorts after "1...".
var_dump("99999999999999999999" > "100000000000000000000");

// <=> is the same ordering, reported as -1, 0 or 1.
var_dump($hi <=> $lo);
echo "a" <=> "b", "\n";
