<?php

// Double-buffered simulation state: two buffers of packed bodies held in properties. Each step
// reads the current frame from one buffer and writes the next frame into the other. A ternary
// or a match picks the buffer, and the pick is the buffer itself (its handle), so reads and
// writes through it reach the chosen buffer without a copy.

packed class Body
{
    public float $x;
    public float $vx;
}

final class World
{
    public buffer<Body> $front;
    public buffer<Body> $back;
    public int $frame = 0;

    public function __construct(int $count)
    {
        $this->front = buffer_new<Body>($count);
        $this->back = buffer_new<Body>($count);
    }

    public function current(): buffer<Body>
    {
        return $this->frame % 2 === 0 ? $this->front : $this->back;
    }

    public function next(): buffer<Body>
    {
        return match ($this->frame % 2) {
            0 => $this->back,
            1 => $this->front,
            default => throw new LogicException("a frame parity is 0 or 1"),
        };
    }

    public function step(float $dt): void
    {
        $from = $this->current();
        $to = $this->next();
        for ($i = 0; $i < buffer_len($from); $i++) {
            $to[$i]->vx = $from[$i]->vx;
            $to[$i]->x = $from[$i]->x + $from[$i]->vx * $dt;
        }
        $this->frame++;
    }
}

$world = new World(3);
for ($i = 0; $i < 3; $i++) {
    $world->front[$i]->x = (float) $i * 10.0;
    $world->front[$i]->vx = (float) $i + 1.0;
}
for ($frame = 0; $frame < 5; $frame++) {
    $world->step(0.5);
}

// Five steps leave the latest frame in the back buffer.
$latest = $world->current();
echo "frame ", $world->frame, ": ";
for ($i = 0; $i < buffer_len($latest); $i++) {
    echo $latest[$i]->x, $i < 2 ? " " : "\n";
}

// A pick can be indexed directly, and a field written through it lands in the chosen buffer.
($world->frame % 2 === 0 ? $world->front : $world->back)[0]->vx = 0.0;
echo "paused body 0: vx=", $world->current()[0]->vx, "\n";

// A buffer is true in a condition. A missing element of an array of buffers is the invalid
// handle 0, which is false, so `?:` falls back to a buffer that exists.
$layers = [$world->front];
$layer = @$layers[1] ?: $world->back;
echo "fallback layer x0=", $layer[0]->x, "\n";
