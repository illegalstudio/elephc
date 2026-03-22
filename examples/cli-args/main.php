<?php
echo "Arguments: " . $argc . "\n";

for ($i = 0; $i < $argc; $i++) {
    echo "  argv[" . $i . "] = " . $argv[$i] . "\n";
}

// Ternary operator
$greeting = $argc > 1 ? "Hello, " . $argv[1] : "Hello, stranger";
echo $greeting . "\n";

// Single-quoted string
echo 'This is a single-quoted string\n (no escaping)' . "\n";

// Built-in functions
$name = "elephc";
echo "strlen: " . strlen($name) . "\n";

$num = "42";
$val = intval($num) + 8;
echo "intval: " . $val . "\n";

// do...while
$n = 1;
do {
    $n *= 2;
} while ($n < 100);
echo "First power of 2 >= 100: " . $n . "\n";
