<?php
packed class EntitySlot { public int $mask; }
packed class Position { public float $x; public float $y; }

final class EntityTable {
    public buffer<EntitySlot> $slots;

    public function __construct(int $count) {
        $this->slots = buffer_new<EntitySlot>($count);
    }

    public function mask(int $index): int { return $this->slots[$index]->mask; }
}

function moveDirect(EntityTable $table, buffer<Position> $positions): void {
    $count = buffer_len($positions);
    for ($index = 0; $index < $count; $index++) {
        if ($table->slots[$index]->mask !== 3) { continue; }
        $positions[$index]->x = $positions[$index]->x + 1.0;
    }
}

function moveInlined(EntityTable $table, buffer<Position> $positions): void {
    $count = buffer_len($positions);
    for ($index = 0; $index < $count; $index++) {
        if ($table->mask($index) !== 3) { continue; }
        $positions[$index]->x = $positions[$index]->x + 1.0;
    }
}

$table = new EntityTable(4);
$positions = buffer_new<Position>(4);
$table->slots[1]->mask = 3;
$table->slots[3]->mask = 3;
moveDirect($table, $positions);
moveInlined($table, $positions);
echo $positions[0]->x, ' ', $positions[1]->x, ' ', $positions[2]->x, ' ', $positions[3]->x, "\n";
$slots = $table->slots;
buffer_free($slots);
buffer_free($positions);
