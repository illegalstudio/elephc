<?php
// Strict comparison (=== and !==)
// Unlike == which compares values with type coercion,
// === checks both type AND value.

// Same type, same value → true
echo "1 === 1: " . (1 === 1 ? "true" : "false") . "\n";

// Same type, different value → false
echo "1 === 2: " . (1 === 2 ? "true" : "false") . "\n";

// Different types → always false, even if values seem equivalent
echo "1 === true: " . (1 === true ? "true" : "false") . "\n";
echo "0 === false: " . (0 === false ? "true" : "false") . "\n";
echo "0 === null: " . (0 === null ? "true" : "false") . "\n";
echo "1.0 === 1: " . (1.0 === 1 ? "true" : "false") . "\n";

// null is only === to null
echo "null === null: " . (null === null ? "true" : "false") . "\n";

// String comparison
echo "\"abc\" === \"abc\": " . ("abc" === "abc" ? "true" : "false") . "\n";
echo "\"abc\" === \"def\": " . ("abc" === "def" ? "true" : "false") . "\n";

// !== is the inverse
echo "1 !== 2: " . (1 !== 2 ? "true" : "false") . "\n";
echo "1 !== 1: " . (1 !== 1 ? "true" : "false") . "\n";

// Practical use: type-safe null check
$x = 0;
if ($x === 0) {
    echo "x is exactly zero\n";
}
