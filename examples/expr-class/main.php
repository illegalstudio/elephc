<?php
// `$expr::class` — the runtime class name of an expression's value (PHP 8.0).
//
// `ClassName::class` is a compile-time constant (the literal written name). `$obj::class`
// instead resolves the *actual* class at runtime: a subclass instance reports its own
// name, not the declared type. elephc lowers `$expr::class` to `get_class($expr)`, so the
// receiver must be an object and the result is the object's real class, matching PHP.

class Shape {}
class Circle extends Shape {}

// A function typed against the base class still observes the runtime class of the value.
function class_of(Shape $o): string {
    return $o::class;
}

$plain = new Shape();
$round = new Circle();

echo class_of($plain) . "\n";   // Shape
echo class_of($round) . "\n";   // Circle — runtime class, not the declared `object` type