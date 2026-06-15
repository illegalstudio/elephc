<?php

// A single untyped closure can be invoked with arguments of different types.
// Each call site keeps its own argument type instead of the closure locking to
// whatever type it was first called with.

$describe = function ($value) {
    if (is_string($value)) {
        return "string(" . strlen($value) . ") \"" . $value . "\"";
    }
    if (is_int($value)) {
        return "int(" . $value . ")";
    }
    if (is_float($value)) {
        return "float(" . $value . ")";
    }
    return "other";
};

echo $describe("hello"), "\n";
echo $describe(42), "\n";
echo $describe(3.5), "\n";

// The same closure also works through the callable indirection helpers.
echo call_user_func($describe, "world"), "\n";
echo call_user_func_array($describe, [128]), "\n";

// A pass-through closure returns its argument unchanged, whatever the type.
$identity = function ($x) { return $x; };
echo $identity("text"), " ", $identity(7), "\n";

// A function can return the result of an untyped closure without it being
// coerced to an integer.
function first(array $items, callable $fn) {
    return $fn($items[0]);
}
echo first(["alpha", "beta"], $identity), "\n";
