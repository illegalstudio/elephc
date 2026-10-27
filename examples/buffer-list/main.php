<?php
// A growable list that copies POD elements without knowing their fields.
final class BufferList<T> {
    public buffer<T> $data;
    public int $length = 0;

    public function __construct() {
        $this->data = buffer_new<T>(1);
    }

    public function pushFrom(buffer<T> $source, int $index): void {
        if ($this->length === buffer_len($this->data)) {
            $old = $this->data;
            $next = buffer_new<T>(2 * buffer_len($old));
            for ($i = 0; $i < $this->length; $i++) {
                $next[$i] = $old[$i];
            }
            $this->data = $next;
            // The caller may be pushing an element of the old buffer itself.
            $this->data[$this->length] = $source[$index];
            buffer_free($old);
        } else {
            $this->data[$this->length] = $source[$index];
        }
        $this->length++;
    }

    public function swapRemove(int $index): void {
        if ($index < 0 || $index >= $this->length) {
            throw new OutOfBoundsException("invalid list index");
        }
        $this->length--;
        $this->data[$index] = $this->data[$this->length];
    }

    public function clear(): void {
        $this->length = 0;
    }

    public function free(): void {
        $data = $this->data;
        buffer_free($data);
        $this->length = 0;
    }
}

packed class Vec2 { public float $x; public float $y; }
$source = buffer_new<Vec2>(1);
$points = new BufferList<Vec2>();
for ($i = 0; $i < 5; $i++) {
    $source[0]->x = (float) $i + 0.5;
    $source[0]->y = 2.0;
    $points->pushFrom($source, 0);
}
$points->swapRemove(0);
$points->swapRemove(0);
echo "points: ", $points->length, " capacity: ", buffer_len($points->data);
echo " first: ", $points->data[0]->x, " last: ", $points->data[2]->x, "\n";

$ids = new BufferList<int>();
$numbers = buffer_new<int>(1);
for ($i = 1; $i <= 3; $i++) {
    $numbers[0] = $i * 10;
    $ids->pushFrom($numbers, 0);
}
$ids->swapRemove(1);
echo "ids: ", $ids->length, " first: ", $ids->data[0], " last: ", $ids->data[1], "\n";
$ids->clear();
echo "cleared: ", $ids->length, "\n";
$ids->free();
$points->free();
buffer_free($source);
buffer_free($numbers);
