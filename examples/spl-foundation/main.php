<?php

// Demonstrates the SPL foundation in elephc:
//   1. user classes implementing built-in SPL/core interfaces
//   2. count() dispatching through Countable
//   3. the SPL exception hierarchy (catch by direct type, by parent
//      LogicException / RuntimeException, or by Exception root)

class Cart implements Countable {
    private array $items = [];

    public function add(string $item): void {
        $this->items[] = $item;
    }

    public function count(): int {
        return count($this->items);
    }
}

$cart = new Cart();
$cart->add("apple");
$cart->add("bread");
$cart->add("cheese");

echo "items: " . count($cart) . PHP_EOL;
echo "Cart instance of Countable? " . (($cart instanceof Countable) ? "yes" : "no") . PHP_EOL;

function require_positive(int $n): void {
    if ($n <= 0) {
        throw new InvalidArgumentException("expected positive, got " . $n);
    }
}

try {
    require_positive(-5);
} catch (LogicException $e) {
    echo "logic error: " . $e->getMessage() . PHP_EOL;
}

try {
    throw new OutOfBoundsException("index 99 out of cart range");
} catch (RuntimeException $e) {
    echo "runtime error: " . $e->getMessage() . PHP_EOL;
}

class StaleSession extends RuntimeException {}

try {
    throw new StaleSession("session expired");
} catch (Exception $e) {
    echo "session: " . $e->getMessage() . PHP_EOL;
}
