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

function queryQuantity(?string $raw, ?int $default = null): int
{
    if ($raw === null && $default !== null) {
        return $default;
    }
    return 0;
}

/** Keeps the completed fallback value in a snapshot created inside its branch. */
function quantitySnapshot(?int $requested, bool $useRequested): Closure
{
    if ($useRequested) {
        $quantity = $requested;
        $quantity ??= 1;
        return fn(): int => $quantity;
    }
    return fn(): int => 1;
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

    public function discountOrDefault(): Money
    {
        $discount = $this->discount;
        if ($discount === null) {
            $discount = new Money(0);
            $this->discount = $discount;
        }
        return $discount; // both exits hold Money, including the cached path
    }

    public function discountAtLeast(int $cents): Money
    {
        $discount = $this->discount;
        if ($discount === null) {
            $discount = new Money($cents);
            $this->discount = $discount;
        } elseif ($discount->cents < $cents) {
            $discount->cents = $cents; // the elseif reads the guard's complement: Money
        }
        return $discount; // every exit of the chain holds Money
    }
}

// An `elseif` chain without `else` joins what each branch stored with what is left once
// every guard was false. Here that remainder is already an array, so `$value` is an
// array after the chain while its slot still holds int|string|array.
function quantities(int|string|array $value): array
{
    if (is_int($value)) {
        $value = [$value];
    } elseif (is_string($value)) {
        $value = explode(",", $value);
    }
    return $value;
}

// `iterable` is array|Traversable: a failed `instanceof Traversable` and a failed
// `!is_array` both leave the array half.
function labels(string|iterable $labels): string
{
    if ($labels instanceof Traversable) {
        $labels = iterator_to_array($labels, false);
    } elseif (!is_array($labels)) {
        $labels = [$labels];
    }
    return implode("+", $labels);
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
echo "fallback ", queryQuantity(null, 5), "\n";
$snapshot = quantitySnapshot(null, true);
echo "snapshot ", $snapshot(), "\n";
echo "discount ", (new Cart(new Money(250)))->requireDiscount()->format(), "\n";
$cart = new Cart(null);
$discount = $cart->discountOrDefault();
echo "default discount ", $discount->format(), "\n";
echo "cached discount ", $discount === $cart->discountOrDefault() ? "same" : "different", "\n";
echo "raised discount ", $cart->discountAtLeast(500)->format(), "\n";
echo "kept discount ", (new Cart(new Money(900)))->discountAtLeast(500)->format(), "\n";
echo "quantities ", count(quantities(3)), count(quantities("1,2")), count(quantities([1, 2, 3])), "\n";
echo "labels ", labels("a"), " ", labels(["b", "c"]), " ", labels(new ArrayIterator(["d"])), "\n";

foreach (metaFiles($dir) as $path) {
    echo "meta ", basename($path), "\n";
}
echo "missing ", count(metaFiles($dir . "/nope")), "\n";
