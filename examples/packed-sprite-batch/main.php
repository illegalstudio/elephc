<?php
// Packed vertex writes and a cold capacity guard inline into the sprite insertion path.
packed class Point { public float $x; public float $y; }
packed class Color { public float $r; public float $g; public float $b; public float $a; }
packed class Vertex { public Point $position; public Color $color; public Point $tex_coord; }

final class SpriteBatch
{
    private buffer<Vertex> $vertices;
    private int $count = 0;
    private int $capacity;
    private float $red = 1.0;
    private float $green = 0.5;
    private float $blue = 0.25;
    private float $alpha = 1.0;

    public function __construct(int $capacity)
    {
        $this->capacity = $capacity;
        $this->vertices = buffer_new<Vertex>($capacity * 4);
    }

    private function reserve(): int
    {
        if ($this->count === $this->capacity) {
            throw new OverflowException('The sprite batch holds at most ' . $this->capacity . ' sprites.');
        }
        return 4 * $this->count++;
    }

    private function vertex(int $vertex, float $x, float $y, float $u, float $v): void
    {
        $this->vertices[$vertex]->position->x = $x;
        $this->vertices[$vertex]->position->y = $y;
        $this->vertices[$vertex]->color->r = $this->red;
        $this->vertices[$vertex]->color->g = $this->green;
        $this->vertices[$vertex]->color->b = $this->blue;
        $this->vertices[$vertex]->color->a = $this->alpha;
        $this->vertices[$vertex]->tex_coord->x = $u;
        $this->vertices[$vertex]->tex_coord->y = $v;
    }

    public function add(float $x, float $y, float $width, float $height): void
    {
        $first = $this->reserve();
        $this->vertex($first, $x, $y, 0.0, 0.0);
        $this->vertex($first | 1, $x + $width, $y, 1.0, 0.0);
        $this->vertex($first | 2, $x + $width, $y + $height, 1.0, 1.0);
        $this->vertex($first | 3, $x, $y + $height, 0.0, 1.0);
    }

    public function inspect(int $vertex): void
    {
        echo $this->vertices[$vertex]->position->x, ',',
            $this->vertices[$vertex]->position->y, ':',
            $this->vertices[$vertex]->color->g, ':',
            $this->vertices[$vertex]->tex_coord->x, ',',
            $this->vertices[$vertex]->tex_coord->y, "\n";
    }

    public function release(): void
    {
        $vertices = $this->vertices;
        buffer_free($vertices);
    }
}

$batch = new SpriteBatch(1);
$batch->add(1.0, 2.0, 8.0, 8.0);
$batch->inspect(0);
$batch->inspect(2);
try {
    $batch->add(3.0, 4.0, 8.0, 8.0);
} catch (OverflowException $error) {
    echo $error->getMessage(), "\n";
}
$batch->release();
