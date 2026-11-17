<?php
// Compile with: elephc --strict-locals examples/strict-locals/main.php

function updateQuantity(mixed $replacement, ?int $quantity = 2): void {
    try {
        $quantity = $replacement;
    } catch (TypeError $error) {
        echo "Quantity requires an integer or null", PHP_EOL;
    }
    echo "Quantity: ", $quantity === null ? "not tracked" : $quantity, PHP_EOL;
}

updateQuantity(2.5);
updateQuantity(3);
updateQuantity(null);

$limit = $argc > 0 ? PHP_INT_MAX : 1;
try {
    $limit += 1;
} catch (TypeError $error) {
    echo "The integer binding kept its value", PHP_EOL;
}

// Ordinary PHP arithmetic still promotes a standalone overflowing result.
echo "Arithmetic result: ", gettype(($argc > 0 ? PHP_INT_MAX : 1) + 1), PHP_EOL;

// Unsetting an ordinary unaliased binding permits a new binding of another kind.
$measurement = $argc;
unset($measurement);
$measurement = 2.5;
echo "Fresh measurement: ", $measurement, PHP_EOL;

// Untyped reference parameters retain the calling variable's binding contract.
function replaceQuantity(&$quantity, mixed $replacement): void {
    $quantity = $replacement;
}

$quantity = $argc;
try {
    replaceQuantity($quantity, 2.5);
} catch (TypeError $error) {
    echo "The referenced quantity kept its integer value", PHP_EOL;
}
