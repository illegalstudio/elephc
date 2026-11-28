<?php
// Compile with: elephc build --strict-casts main.php

function readCount(mixed $value): int {
    return (int) $value;
}

echo "Count: ", readCount("42.9"), "\n";
echo "Exact maximum: ", readCount("9223372036854775807.9"), "\n";

$input = $argc > 0 ? "12items" : "invalid";
try {
    echo readCount($input);
} catch (ValueError $error) {
    echo "Count requires a numeric string\n";
}

$amount = $argc > 0 ? "1.25e2" : "0";
echo "Amount: ", (float) $amount, "\n";
