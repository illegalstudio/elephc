<?php
final class LoopSlots {
    private buffer<int> $slots;

    public function __construct() { $this->slots = buffer_new<int>(4); }

    public function fill(int $count): void {
        for ($index = 0; $index < $count; $index++) {
            $this->slots[$index] = 7;
        }
    }

    public function at(int $index): int { return $this->slots[$index]; }

    public function clear(): void {
        $slots = $this->slots;
        buffer_free($slots);
    }
}

$slots = new LoopSlots();
$slots->fill($argc + 2);
echo $slots->at(0), $slots->at(1), $slots->at(2), $slots->at(3);
$slots->clear();
