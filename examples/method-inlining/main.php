<?php
// Small accessor and predicate methods on a final class, called in a hot loop.
//
// The optimizer splices a direct call to a small method it can bind statically
// (final class, final method, or a method nothing can override) into the caller,
// so engine code can keep its accessors without paying for a call per entity.
// The spliced body still reads the private property from the method's own scope,
// and a null receiver still raises "Call to a member function ... on null".
//
// Usage: ./main [entities]

packed class Slot
{
    public bool $alive;
    public int $mask;
}

final class Table
{
    private buffer<Slot> $slots;
    private int $highWater = 0;

    public function __construct(int $capacity)
    {
        $this->slots = buffer_new<Slot>($capacity);
    }

    public function add(int $mask): void
    {
        $index = $this->highWater;
        $this->slots[$index]->alive = true;
        $this->slots[$index]->mask = $mask;
        $this->highWater = $index + 1;
    }

    // Buffers have no automatic cleanup: the table releases its storage explicitly.
    public function release(): void
    {
        $slots = $this->slots;
        buffer_free($slots);
    }

    public function highWater(): int
    {
        return $this->highWater;
    }

    public function matches(int $index, int $components): bool
    {
        return $this->slots[$index]->alive
            && ($this->slots[$index]->mask & $components) === $components;
    }
}

const POSITION = 1;
const VELOCITY = 2;

// The system lives in a function: top-level variables would be PHP globals.
function countMoving(Table $table): int
{
    $moving = 0;
    $slots = $table->highWater();
    for ($i = 0; $i < $slots; $i++) {
        if (!$table->matches($i, POSITION | VELOCITY)) {
            continue;
        }
        $moving++;
    }
    return $moving;
}

$count = (int) ($argv[1] ?? 1000);
$table = new Table($count);
for ($i = 0; $i < $count; $i++) {
    $table->add($i % 4 === 3 ? POSITION : POSITION | VELOCITY);
}

echo "entities: ", $table->highWater(), "\n";
echo "moving: ", countMoving($table), "\n";
$table->release();
