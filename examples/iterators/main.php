<?php

// Demonstrates implementing the built-in Iterator interface and consuming
// it from foreach and iterable-typed functions.

class Range implements Iterator {
    private int $current;
    private int $end;
    private int $step;

    public function __construct(int $start, int $end, int $step) {
        $this->current = $start;
        $this->end = $end;
        $this->step = $step;
    }

    public function rewind(): void {
        // Range has nothing to reset — `current` was set by __construct.
    }

    public function valid(): bool {
        return $this->current < $this->end;
    }

    public function current(): mixed {
        return $this->current;
    }

    public function key(): mixed {
        return $this->current;
    }

    public function next(): void {
        $this->current = $this->current + $this->step;
    }
}

echo "range 0..5 step 1:\n";
foreach (new Range(0, 5, 1) as $i) {
    echo $i;
    echo " ";
}
echo "\n";

echo "range 10..20 step 3:\n";
foreach (new Range(10, 20, 3) as $i) {
    echo $i;
    echo " ";
}
echo "\n";

echo "early exit when current >= 5:\n";
foreach (new Range(1, 100, 1) as $i) {
    if ($i == 5) {
        echo $i;
        echo "\n";
        break;
    }
}

// IteratorAggregate: the object itself isn't an iterator — its
// getIterator() method returns one. foreach calls getIterator() once
// before iterating.

class RangeFactory implements IteratorAggregate {
    private int $start;
    private int $end;

    public function __construct(int $start, int $end) {
        $this->start = $start;
        $this->end = $end;
    }

    public function getIterator(): Iterator {
        return new Range($this->start, $this->end, 1);
    }
}

echo "iterator aggregate 0..3:\n";
foreach (new RangeFactory(0, 3) as $i) {
    echo $i;
    echo " ";
}
echo "\n";

function print_any(iterable $items): void {
    foreach ($items as $key => $value) {
        echo $key;
        echo ":";
        echo $value;
        echo " ";
    }
    echo "\n";
}

function print_reindexed(iterable $items, bool $preserve): void {
    $values = iterator_to_array($items, $preserve);
    foreach ($values as $key => $value) {
        echo $key;
        echo "=";
        echo $value;
        echo " ";
    }
    echo "\n";
}

echo "iterable parameter from Iterator:\n";
print_any(new Range(2, 5, 1));

echo "iterable parameter from IteratorAggregate:\n";
print_any(new RangeFactory(4, 7));

echo "iterator_to_array from iterable parameter:\n";
$preserveKeys = false;
print_reindexed(["a" => 10, "b" => 20], $preserveKeys);

echo is_iterable(new Range(0, 1, 1)) ? "iterator is iterable\n" : "not iterable\n";
echo is_iterable(new RangeFactory(0, 1)) ? "aggregate is iterable\n" : "not iterable\n";

echo "iterator_to_array without keys:\n";
$values = iterator_to_array(new Range(0, 3, 1), false);
foreach ($values as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo " ";
}
echo "\n";

echo "iterator_count from aggregate:\n";
echo iterator_count(new RangeFactory(0, 4));
echo "\n";

function iterator_tick(string $label): bool {
    echo $label;
    return true;
}

echo "iterator_apply callback count:\n";
$labels = ["*"];
echo iterator_apply(new Range(0, 3, 1), "iterator_tick", $labels);
echo "\n";

function iterator_label_once(): string {
    echo "!";
    return "+";
}

echo "iterator_apply expression args:\n";
echo iterator_apply(new Range(0, 2, 1), "iterator_tick", [iterator_label_once()]);
echo "\n";

function make_iterator_labeler(): callable {
    return function(string $label): bool {
        echo $label;
        return true;
    };
}

echo "iterator_apply returned callable:\n";
$dynamic_labels = ["?"];
echo iterator_apply(new Range(0, 2, 1), make_iterator_labeler(), $dynamic_labels);
echo "\n";
