<?php

// References: aliasing storage so two names share one value.
//
// A reference (`$x = &<source>`) makes the target an alias of the source. Writing
// through either name is observed through the other. elephc supports aliasing plain
// variables, object properties, and the results of by-reference function/method returns.

// --- Aliasing a plain variable ---
$a = 1;
$b = &$a;
$b = 5;            // writes through to $a
echo $a, "\n";     // 5

// --- Aliasing an object property (write-through both ways) ---
class Counter
{
    public int $value = 0;
}

$counter = new Counter();
$ref = &$counter->value;
$ref = 10;                 // updates the property through the alias
echo $counter->value, "\n"; // 10
$counter->value = 42;       // updates the alias through the property
echo $ref, "\n";            // 42

// --- A reference into an array property ---
class Bag
{
    public array $items = [];
}

$bag = new Bag();
$items = &$bag->items;
$items[] = "apple";
$items[] = "banana";
echo implode(", ", $bag->items), "\n"; // apple, banana

// --- By-reference function return, captured with `$x = &f()` ---
function &firstSlot(Counter $c)
{
    return $c->value; // returns a reference to the property
}

$slot = &firstSlot($counter);
$slot = 99;
echo $counter->value, "\n"; // 99

// --- By-reference method return ---
class Registry
{
    public array $entries = [];

    public function &all()
    {
        return $this->entries;
    }
}

$registry = new Registry();
$entries = &$registry->all();
$entries[] = "first";
$entries[] = "second";
echo implode(", ", $registry->entries), "\n"; // first, second

// --- A by-reference closure bound to an object, capturing a reference to its property ---
// (the shape Symfony's Kernel uses to obtain and later clear a live reference)
class Container
{
    public array $services = [];
}

$container = new Container();
$services = &Closure::bind(fn &() => $this->services, $container, $container)();
$services[] = "logger";
echo implode(", ", $container->services), "\n"; // logger
$services = [];                                  // clears through the reference
echo count($container->services), "\n";          // 0

// --- The bound closure can also be stored and called later ---
$accessor = Closure::bind(fn &() => $this->services, $container, $container);
$later = &$accessor();
$later[] = "router";
echo implode(", ", $container->services), "\n"; // router

// --- A reference to a string property (any scalar type works) ---
class Label
{
    public string $text = "draft";
}

$label = new Label();
$text = &$label->text;
$text = "final";                 // writes through to the property
echo $label->text, "\n";         // final

// --- Reassigning an array reference to a typed literal keeps it readable ---
$bag = new Bag();
$entries = &$bag->items;
$entries = [10, 20, 30];         // elements are boxed to match the property
echo implode(", ", $bag->items), "\n"; // 10, 20, 30
