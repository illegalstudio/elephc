<?php
// Classes — constructors, methods, properties

class Counter {
    const STEP = 1;
    const TRIPLE_STEP = self::STEP * 3;

    public $count;

    public function __construct() {
        $this->count = 0;
    }

    public function inc() {
        $this->count += self::STEP;
    }

    public function dec() {
        if ($this->count > 0) {
            $this->count -= 1;
        }
    }

    public function get() {
        return $this->count;
    }

    public function show() {
        echo "Count: " . $this->count . "\n";
    }
}

// Create and use a counter
$c = new Counter();
$c->show();

$c->inc();
$c->inc();
$c->inc();
$c->show();
echo "Triple step: " . Counter::TRIPLE_STEP . "\n";

$c->dec();
$c->show();

echo "Final value: " . $c->get() . "\n";

// Multiple instances are independent
$a = new Counter();
$b = new Counter();
$a->inc();
$a->inc();
$b->inc();
echo "a=" . $a->get() . " b=" . $b->get() . "\n";
