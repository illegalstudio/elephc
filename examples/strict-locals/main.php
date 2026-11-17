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

// A later valid assignment cannot cancel an earlier rejected write inside literal eval.
function checkLiteralLabel(): void {
    $label = 'Quantity';
    try {
        eval('$label = 1; $label = "Quantity";');
    } catch (TypeError $error) {
        echo "The literal eval kept the label value", PHP_EOL;
    }
}
checkLiteralLabel();

// A binding created inside eval retains its own type until it is unset.
function checkLiteralCreatedQuantity(): void {
    try {
        eval('$quantity = 1;');
        $quantity = 2.5;
    } catch (TypeError $error) {
        echo "The eval-created quantity kept its integer type", PHP_EOL;
    }
}
checkLiteralCreatedQuantity();

// Runtime-selected source preserves the same contract when execution returns to native code.
function checkRuntimeCreatedQuantity(int $requestCount): void {
    $source = $requestCount > 0 ? '$quantity = 1;' : '$quantity = 0;';
    eval($source);
    try {
        $quantity = 2.5;
    } catch (TypeError $error) {
        echo "The runtime-created quantity kept its integer type", PHP_EOL;
    }
}
checkRuntimeCreatedQuantity($argc);

// Both alternatives of a producer stay valid when the runtime selects the first value.
function checkRuntimeProducerAlternatives(int $requestCount): void {
    $source = $requestCount > 0
        ? '$measurement = $requestCount > 0 ? 1 : 2.5; $measurement = 2.5;'
        : '$measurement = 2.5;';
    eval($source);
    echo "Runtime measurement: ", $measurement, PHP_EOL;
}
checkRuntimeProducerAlternatives($argc);

// An unexecuted native declaration does not constrain a fresh eval binding.
function checkConditionalQuantity(bool $known): void {
    if ($known) {$quantity=1;}
    try {
        eval('$quantity=2.5;');
        echo "Fresh conditional quantity: ", $quantity, PHP_EOL;
    } catch (TypeError $error) {
        echo "The conditional quantity kept its integer type", PHP_EOL;
    }
}
checkConditionalQuantity(true);
checkConditionalQuantity(false);

// Unsetting and recreating a quantity inside eval gives later native writes its new contract.
function replaceRuntimeQuantity(mixed $replacement, int $requestCount): void {
    $quantity=1;
    $source=$requestCount>0?'unset($quantity); $quantity=2.5;':'';
    eval($source);
    try {
        $quantity=$replacement;
    } catch (TypeError $error) {
        echo "The runtime quantity kept its float value", PHP_EOL;
    }
    echo "Runtime quantity: ", $quantity, PHP_EOL;
}
replaceRuntimeQuantity(3.5, $argc);
replaceRuntimeQuantity(1, $argc);

function copyEvalQuantity(int $requestCount): void {
    eval('$quantity = 2.5;');
    $copy = $quantity;
    try {
        $copy = $requestCount;
    } catch (TypeError $error) {
        echo "The copied quantity kept its float value", PHP_EOL;
    }
    echo "Copied quantity: ", $copy, PHP_EOL;
}
copyEvalQuantity($argc);

// Callable bindings keep their capability, including writes through a reference.
function checkQuantityCallback(mixed $replacement): void {
    $format = static fn(int $quantity): string => "Quantity: " . strval($quantity);
    try {
        replaceValue($format, $replacement);
    } catch (TypeError $error) {
        echo "The callback kept its callable contract", PHP_EOL;
    }
    echo $format(3), PHP_EOL;
}
checkQuantityCallback('missingQuantityFormatter');
