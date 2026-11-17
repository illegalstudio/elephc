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
function replaceValue(&$target, mixed $replacement): void {
    $target = $replacement;
}

$quantity = $argc;
try {
    replaceValue($quantity, 2.5);
} catch (TypeError $error) {
    echo "The referenced quantity kept its integer value", PHP_EOL;
}

$label = 'Quantity';
try {
    replaceValue($label, 2);
} catch (TypeError $error) {
    echo "The label kept its string value", PHP_EOL;
}

// Explicit conversion does not remove a binding's independent value contract.
try {
    settype($label, 'int');
} catch (TypeError $error) {
    echo "The conversion kept the label value", PHP_EOL;
}

// Literal eval retains the contract of a binding imported from native code.
function checkLiteralLabel(): void {
    $label = 'Quantity';
    try {
        eval('$label = 1;');
    } catch (TypeError $error) {
        echo "The literal eval kept the label value", PHP_EOL;
    }
}
checkLiteralLabel();
