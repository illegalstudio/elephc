<?php

// A value object that returns `self` from its operations for fluent chaining.
final class Money
{
    public function __construct(public int $cents) {}

    public function add(self $other): self
    {
        return new Money($this->cents + $other->cents);
    }

    public function cents(): int
    {
        return $this->cents;
    }
}

// A factory using a `static` return type plus `self`-returning fluent mutators.
class Counter
{
    public int $n = 0;

    public static function start(): static
    {
        return new static();
    }

    public function inc(): self
    {
        $this->n = $this->n + 1;
        return $this;
    }
}

// A singly linked list node using a nullable `?self` property.
class Node
{
    public ?self $next = null;

    public function __construct(public int $value) {}
}

$total = (new Money(150))->add(new Money(350))->add(new Money(99));
echo $total->cents(), "\n";

$counter = Counter::start()->inc()->inc()->inc();
echo $counter->n, "\n";

$head = new Node(1);
$head->next = new Node(2);
$head->next->next = new Node(3);

// Read through the nullable `?self` links.
echo $head->value + $head->next->value + $head->next->next->value, "\n";
echo $head->next->next->next === null ? "tail\n" : "more\n";
