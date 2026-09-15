<?php

// Type narrowing: predicates, strict literal checks, and `instanceof` guards
// refine variables and stable properties without explicit casts.

class Money
{
    public int $cents = 0;

    public function __construct(int $cents)
    {
        $this->cents = $cents;
    }

    public function format(): string
    {
        return "$" . intdiv($this->cents, 100) . "." . ($this->cents % 100);
    }
}

// `$value` is inferred as int|Money from the two call sites below.
function render($value): string
{
    if (is_int($value)) {
        return "qty " . ($value + 1);          // $value is int here -> arithmetic
    }
    if ($value instanceof Money) {
        return "price " . $value->format();     // $value is Money here -> method call
    }
    return "unknown";
}

function requireQuantity(int|false $value): int
{
    if ($value === false) {
        throw new InvalidArgumentException("quantity unavailable");
    }
    return $value; // int|false has narrowed to int
}

class Cart
{
    public function __construct(public ?Money $discount)
    {
    }

    public function requireDiscount(): Money
    {
        if (is_null($this->discount)) {
            throw new LogicException("discount missing");
        }
        return $this->discount; // the stable property has narrowed to Money
    }
}

// A guard narrows what the branch READS. An assignment inside it is still measured
// against the variable's own type, so the standard fallback idiom compiles: `$found`
// reads as `false` in the branch, yet `string[]|false` holds an empty array perfectly
// well.
function metaFiles(string $dir): array
{
    $found = glob($dir . "/*.meta");
    if ($found === false) {
        $found = [];
    }
    sort($found);
    return $found;
}

$dir = sys_get_temp_dir() . "/elephc-narrowing-example";
if (!is_dir($dir)) {
    mkdir($dir);
}
file_put_contents($dir . "/beta.meta", "b");
file_put_contents($dir . "/alpha.meta", "a");

echo render(7), "\n";
echo render(new Money(1299)), "\n";
echo "quantity ", requireQuantity(3), "\n";
echo "discount ", (new Cart(new Money(250)))->requireDiscount()->format(), "\n";

foreach (metaFiles($dir) as $path) {
    echo "meta ", basename($path), "\n";
}
echo "missing ", count(metaFiles($dir . "/nope")), "\n";
