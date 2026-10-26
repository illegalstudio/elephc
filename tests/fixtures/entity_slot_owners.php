<?php
packed class Slot
{
    public int $generation;
    public int $nextFree;
    public int $mask;
    public bool $alive;
}

final class Table
{
    private buffer<Slot> $slots;
    private int $freeHead = -1;
    private int $highWater = 0;
    private int $count = 0;

    public function __construct(private readonly int $capacity)
    {
        $this->slots = buffer_new<Slot>($capacity);
    }

    public function __destruct()
    {
        $slots = $this->slots;
        buffer_free($slots);
    }

    public function create(): int
    {
        if ($this->freeHead !== -1) {
            $index = $this->freeHead;
            $this->freeHead = $this->slots[$index]->nextFree;
        } elseif ($this->highWater < $this->capacity) {
            $index = $this->highWater;
            $this->highWater++;
            $this->slots[$index]->generation = 1;
        } else {
            throw new OverflowException('Full.');
        }
        $this->slots[$index]->alive = true;
        $this->count++;
        return ($this->slots[$index]->generation << 32) | $index;
    }
}

function run(int $n): void
{
    $table = new Table($n);
    for ($i = 0; $i < $n; $i++) {
        $entity = $table->create();
    }
    echo "last ", $entity, "\n";
    unset($table);
}
