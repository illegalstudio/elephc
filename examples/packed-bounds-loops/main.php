<?php
// Sprites cover rectangles of a small tile grid. Each rectangle's inclusive bounds live in
// a packed record, and the tile loops start from those fields. The counters stay boxed
// (an inclusive `<=` bound could overflow to float in PHP), but their integer seeds and
// increments use frame-local cells, so re-entering the loops allocates nothing.

packed class TileRect {
    public int $left;
    public int $top;
    public int $right;
    public int $bottom;
}

function paint(buffer<TileRect> $rects, buffer<int> $tiles, int $width): void {
    for ($i = 0; $i < buffer_len($rects); $i++) {
        for ($y = $rects[$i]->top; $y <= $rects[$i]->bottom; $y++) {
            for ($x = $rects[$i]->left; $x <= $rects[$i]->right; $x++) {
                $tiles[$y * $width + $x]++;
            }
        }
    }
}

$width = 6;
$tiles = buffer_new<int>($width * 4);
$rects = buffer_new<TileRect>(3);
$layout = [[0, 0, 2, 1], [1, 1, 4, 2], [3, 0, 5, 3]];
foreach ($layout as $i => [$left, $top, $right, $bottom]) {
    $rects[$i]->left = $left;
    $rects[$i]->top = $top;
    $rects[$i]->right = $right;
    $rects[$i]->bottom = $bottom;
}

$before = memory_get_allocations();
paint($rects, $tiles, $width);
$allocations = memory_get_allocations() - $before;

// Prints the coverage count of every tile:
// 111111
// 122221
// 011221
// 000111
for ($row = 0; $row < 4; $row++) {
    for ($column = 0; $column < $width; $column++) { echo $tiles[$row * $width + $column]; }
    echo "\n";
}
// Prints: allocations while painting: 0
echo "allocations while painting: ", $allocations, "\n";
buffer_free($rects);
buffer_free($tiles);
