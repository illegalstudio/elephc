<?php
// Compile with: elephc --strict-dynamic-code --strict-bools examples/strict-semantics/main.php
// Literal source is verified and compiled ahead of time.
eval('echo "Verified at compilation", PHP_EOL;');

function showDecision(mixed $value): void {
    try {
        if ($value) {
            echo "Accepted a boolean", PHP_EOL;
        }
    } catch (TypeError $error) {
        echo "Condition requires bool", PHP_EOL;
    }
}

showDecision(true);
showDecision(1);
if ((bool) $argc) {
    echo "Explicit conversion", PHP_EOL;
}
