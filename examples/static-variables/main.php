<?php

// A `static` variable keeps its value between calls. Several can be declared in one statement,
// separated by commas, each with its own initializer.

// A running tally with two independent persistent counters declared together.
function record(bool $hit): string
{
    static $hits = 0, $misses = 0;
    if ($hit) {
        $hits = $hits + 1;
    } else {
        $misses = $misses + 1;
    }
    return "hits=$hits misses=$misses";
}

echo record(true), "\n";   // hits=1 misses=0
echo record(true), "\n";   // hits=2 misses=0
echo record(false), "\n";  // hits=2 misses=1

// A classic use: memoize an expensive computation so it runs once per input is not needed here,
// but the persistence makes a simple call counter trivial.
function next_id(): int
{
    static $id = 0;
    $id = $id + 1;
    return $id;
}

echo next_id(), next_id(), next_id(), "\n"; // 123
