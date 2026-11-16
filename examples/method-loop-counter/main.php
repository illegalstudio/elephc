<?php
final class Box {
    public function size(): int { return 10000; }
    public function hit(int $index): bool { return ($index & 3) !== 3; }
}

function countHits(Box $box, int $rounds): int {
    $hits = 0;
    for ($round = 0; $round < $rounds; $round++) {
        $size = $box->size();
        for ($index = 0; $index < $size; $index++) {
            if ($box->hit($index)) { $hits++; }
        }
    }
    return $hits;
}

echo countHits(new Box(), $argc * 3);
