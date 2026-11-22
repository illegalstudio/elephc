<?php
// Compile with: elephc build --strict-locals examples/strict-locals/main.php

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

// A resource-or-false producer keeps both alternatives without accepting an integer.
function checkQuantityStream(mixed $replacement): void {
    $stream=tmpfile();
    $owner=$stream;
    try {
        $stream=$replacement;
    } catch (TypeError $error) {
        echo "The stream kept its resource contract", PHP_EOL;
    }
    fclose($owner);
}
checkQuantityStream(1);

// Exclusive first bindings keep their executed contract through a captured reference.
function replaceSelectedValue(bool $numeric, mixed $replacement): void {
    if ($numeric) { $value = 1; } else { $value = 'pending'; }
    $replace = function(mixed $replacement) use (&$value): void {
        replaceValue($value, $replacement);
    };
    try {
        $replace($replacement);
    } catch (TypeError $error) {
        echo "The selected binding kept its original kind", PHP_EOL;
    }
    echo "Selected value: ", $value, PHP_EOL;
}
replaceSelectedValue(true, 2.5);
replaceSelectedValue(false, 'ready');

// A captured reference detaches locally before creating a new producer.
function resetSelectedValue(bool $reset, int &$value): void {
    $select = function(bool $reset) use (&$value): void {
        if ($reset) { unset($value); $value = 'reset'; }
        eval('');
        echo "Current value: ", $value, PHP_EOL;
    };
    $select($reset);
}
$resetQuantity = 1;
resetSelectedValue(true, $resetQuantity);
resetSelectedValue(false, $resetQuantity);

// A new reference after unset is independent, including in a frame that exposes locals to eval.
function recreateQuantityReference(): void {
    $value = 1;
    unset($value);
    $read = function() use (&$value): mixed { return $value; };
    $value = 'ready';
    eval('');
    echo "Recreated reference: ", $read(), PHP_EOL;
}
recreateQuantityReference();

// An unannotated input keeps its open contract when a reference argument changes its value.
function createQuantityArgument($newQuantity): void {
    replaceValue($newQuantity, 'ready');
    echo "Reference argument: ", $newQuantity, PHP_EOL;
}
createQuantityArgument(1);

// A method can initialize a fresh label through the same reference argument rules.
class QuantityLabels {
    public function initialize(mixed &$label): void { $label = 'Quantity'; }
}
$labels = new QuantityLabels();
$labels->initialize($newLabel);
echo "Reference label: ", $newLabel, PHP_EOL;

// A value snapshot keeps its selected contract after capture, independently of its source.
function checkQuantitySnapshot(bool $numeric, mixed $replacement): void {
    if ($numeric) { $value = 1; } else { $value = 'pending'; }
    $replace = function(mixed $replacement) use ($value): mixed {
        replaceValue($value, $replacement);
        return $value;
    };
    try {
        $updated = $replace($replacement);
        echo "Snapshot value: ", $updated, PHP_EOL;
    } catch (TypeError $error) {
        echo "The snapshot kept its original kind", PHP_EOL;
    }
    echo "Snapshot original: ", $value, PHP_EOL;
}
checkQuantitySnapshot(true, 2.5);
checkQuantitySnapshot(false, 'ready');

// A global value snapshot keeps its owner when the program symbol is replaced.
$globalQuantity = 1;
function snapshotGlobalQuantity(): callable {
    global $globalQuantity;
    return function() use ($globalQuantity): mixed { return $globalQuantity; };
}
$globalSnapshot = snapshotGlobalQuantity();
$globalQuantity = 2;
echo "Global snapshot: ", $globalSnapshot(), PHP_EOL;
echo "Global quantity: ", $globalQuantity, PHP_EOL;

// Detaching a static name leaves the persistent quantity available to the next call.
function selectStoredQuantity(bool $reset): void {
    static $quantity = 1;
    if ($reset) { unset($quantity); $quantity = 'not tracked'; }
    echo "Stored quantity: ", $quantity, PHP_EOL;
}
selectStoredQuantity(true);
selectStoredQuantity(false);

// A detached global name becomes local while the program keeps its original quantity.
function detachGlobalQuantity(bool $detach): void {
    global $globalQuantity;
    if ($detach) { unset($globalQuantity); }
    try {
        $globalQuantity = 'not tracked';
        echo "Detached quantity: ", $globalQuantity, PHP_EOL;
    } catch (TypeError $error) {
        echo "The global quantity kept its integer type", PHP_EOL;
    }
}
detachGlobalQuantity(true);
detachGlobalQuantity(false);
echo "Retained global quantity: ", $globalQuantity, PHP_EOL;
