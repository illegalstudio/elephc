<?php
// Particle system: a class that owns a buffer of packed records.
//
// `buffer<T>` and packed class names work in every class member signature:
// properties, method parameters and return types, promoted constructor
// parameters, and generic type arguments.

packed class Particle {
    public float $x;
    public float $y;
    public float $vx;
    public float $vy;
}

final class ParticleSystem {
    public buffer<Particle> $particles;
    public buffer<int> $spawnOrder;
    private int $count = 0;

    public function __construct(int $capacity) {
        $this->particles = buffer_new<Particle>($capacity);
        $this->spawnOrder = buffer_new<int>($capacity);
    }

    public function spawn(float $x, float $y, float $vx, float $vy): void {
        $i = $this->count;
        $this->particles[$i]->x = $x;
        $this->particles[$i]->y = $y;
        $this->particles[$i]->vx = $vx;
        $this->particles[$i]->vy = $vy;
        $this->spawnOrder[$i] = $i + 1;
        $this->count = $i + 1;
    }

    public function step(float $dt): void {
        for ($i = 0; $i < $this->count; $i++) {
            $this->particles[$i]->x = $this->particles[$i]->x + $this->particles[$i]->vx * $dt;
            $this->particles[$i]->y = $this->particles[$i]->y + $this->particles[$i]->vy * $dt;
        }
    }

    public function count(): int {
        return $this->count;
    }

    public function snapshot(): buffer<Particle> {
        return $this->particles;
    }
}

// A generic container instantiated with a packed element type.
final class Ring<T> {
    public function __construct(public buffer<T> $slots) {}

    public function capacity(): int {
        return buffer_len($this->slots);
    }
}

function describe(buffer<Particle> $particles, int $count): void {
    for ($i = 0; $i < $count; $i++) {
        echo "particle ", $i, ": ", $particles[$i]->x, ",", $particles[$i]->y, "\n";
    }
}

$system = new ParticleSystem(8);
$system->spawn(0.0, 0.0, 1.0, 2.0);
$system->spawn(10.0, 5.0, -0.5, 0.25);
$system->spawn(-3.0, 4.0, 0.0, -1.0);
for ($frame = 0; $frame < 4; $frame++) {
    $system->step(0.5);
}
describe($system->snapshot(), $system->count());
echo "spawn order: ", $system->spawnOrder[0], ",", $system->spawnOrder[1], ",", $system->spawnOrder[2], "\n";

$ring = new Ring<Particle>(buffer_new<Particle>(16));
echo "ring capacity: ", $ring->capacity(), "\n";

// Buffers have no automatic cleanup: free each one once nothing reads it any more.
buffer<int> $spawnOrder = $system->spawnOrder;
buffer_free($spawnOrder);
buffer<Particle> $particles = $system->snapshot();
buffer_free($particles);
buffer<Particle> $slots = $ring->slots;
buffer_free($slots);
