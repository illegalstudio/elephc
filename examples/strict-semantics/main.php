<?php
// Compile with: elephc --strict-dynamic-code --strict-bools examples/strict-semantics/main.php
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
