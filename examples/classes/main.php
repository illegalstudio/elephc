<?php
// Classes — constructors, methods, properties

class Counter {
    const int STEP = 1;
    const int TRIPLE_STEP = self::STEP * 3;

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

    public static function hasStep() {
        return defined('static::STEP');
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
echo defined('Counter::STEP') ? "defined STEP\n" : "missing STEP\n";
echo Counter::hasStep() ? "late-static STEP\n" : "missing late-static STEP\n";
echo defined('Counter::MISSING') ? "defined MISSING\n" : "missing MISSING\n";

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

// One declaration can introduce several properties or constants, separated by commas. The type
// and every modifier belong to the whole list; each name carries its own initializer.
class Viewport
{
    const int MIN_WIDTH = 40, MIN_HEIGHT = 22;

    public int $width = 80, $height = 24;
    private static int $instances = 0, $resizes = 0;

    public function __construct()
    {
        self::$instances++;
    }

    public function shrink_to_minimum(): void
    {
        $this->width = self::MIN_WIDTH;
        $this->height = self::MIN_HEIGHT;
        self::$resizes++;
    }

    public static function tally(): string
    {
        return self::$instances . " built, " . self::$resizes . " resized";
    }
}

$viewport = new Viewport();
echo "Viewport: " . $viewport->width . "x" . $viewport->height . "\n";
$viewport->shrink_to_minimum();
echo "Shrunk to: " . $viewport->width . "x" . $viewport->height . "\n";
echo "Tally: " . Viewport::tally() . "\n";
