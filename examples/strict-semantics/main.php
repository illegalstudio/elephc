<?php
// Compile with: elephc --strict-dynamic-code --strict-bools --strict-arithmetic --strict-overflow examples/strict-semantics/main.php
// Literal source is verified and compiled ahead of time.
eval('echo "Verified at compilation", PHP_EOL;');

const DEFAULT_DECISION = true;

function showDecision(mixed $value = DEFAULT_DECISION): void {
    try {
        if ($value) {
            echo "Accepted a boolean", PHP_EOL;
        }
    } catch (TypeError $error) {
        echo "Condition requires bool", PHP_EOL;
    }
}

showDecision();
showDecision(true);
showDecision(1);
if ((bool) $argc) {
    echo "Explicit conversion", PHP_EOL;
}

$attempts = (int) '3';
echo "Attempts: ", $attempts, PHP_EOL;
$ratio = floatval('1.5');
echo "Ratio: ", $ratio, PHP_EOL;
$timeout = '30';
$convert = settype(...);
$convert($timeout, 'int');
echo "Timeout: ", $timeout, PHP_EOL;

function showSettings(mixed $settings): void {
    $convert = settype(...);
    $convert($settings, 'object');
    echo "Settings timeout: ", $settings->timeout, PHP_EOL;
}

showSettings(['timeout' => $timeout]);

$display = $argc > 0 ? 'strval' : 'STRVAL';
echo "Displayed timeout: ", $display($timeout), PHP_EOL;

function numericTotal(mixed $left, mixed $right): mixed {
    return $left + $right;
}

echo "Division result: ", gettype(4 / 2), PHP_EOL;
try {
    numericTotal('2', 3);
} catch (TypeError $error) {
    echo "Arithmetic requires numeric values", PHP_EOL;
}

try {
    numericTotal(PHP_INT_MAX, 1);
} catch (ArithmeticError $error) {
    echo "Integer overflow was rejected", PHP_EOL;
}
