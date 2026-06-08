<?php

// Type narrowing: `is_*` and `instanceof` guards let an untyped value be used
// as a specific type inside the guarded branch — without an explicit cast.

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

echo render(7), "\n";
echo render(new Money(1299)), "\n";
