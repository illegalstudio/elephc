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

// A property default may be an array literal whose elements are array literals,
// to any depth and in either spelling. Each nested container belongs to the
// object holding it, so two instances never share one.
class Grid {
    public array $rows = [[1, 2], [3, 4]];
    public array $conf = ['db' => ['host' => 'localhost', 'port' => 5432]];

    public function cell(int $row, int $col): int {
        return $this->rows[$row][$col];
    }
}

$g = new Grid();
echo "cell(1,0)=" . $g->cell(1, 0) . "\n";
echo "host=" . $g->conf['db']['host'] . ":" . $g->conf['db']['port'] . "\n";

$g->rows[0][] = 9;
$fresh = new Grid();
echo "written=" . count($g->rows[0]) . " fresh=" . count($fresh->rows[0]) . "\n";
