<?php

// An entity table that owns its packed slots as a private buffer<T> property.
// With --ir-opt (the default) the two $this->slots[$index] reads in matches()
// read the property and resolve the buffer handle once, and the second read
// reuses the record address of the first.

packed class Slot
{
    public int $mask;
    public bool $alive;
}

final class Entities
{
    private buffer<Slot> $slots;
    private int $highWater = 0;

    public function __construct(int $capacity)
    {
        $this->slots = buffer_new<Slot>($capacity);
    }

    public function create(int $mask): int
    {
        $index = $this->highWater;
        $this->highWater = $index + 1;
        $this->slots[$index]->alive = true;
        $this->slots[$index]->mask = $mask;
        return $index;
    }

    public function destroy(int $index): void
    {
        $this->slots[$index]->alive = false;
        $this->slots[$index]->mask = 0;
    }

    public function matches(int $index, int $components): bool
    {
        return $this->slots[$index]->alive
            && ($this->slots[$index]->mask & $components) === $components;
    }

    public function highWater(): int
    {
        return $this->highWater;
    }
}

const POSITION = 1;
const VELOCITY = 2;

$entities = new Entities(16);
for ($i = 0; $i < 10; $i++) {
    $entities->create($i % 3 === 0 ? POSITION : POSITION | VELOCITY);
}
$entities->destroy(4);

$moving = 0;
for ($i = 0; $i < $entities->highWater(); $i++) {
    if ($entities->matches($i, POSITION | VELOCITY)) {
        $moving++;
    }
}
echo "entities=", $entities->highWater(), "; moving=", $moving, "\n";
