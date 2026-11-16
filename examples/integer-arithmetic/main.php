<?php
// Pixel offsets can be computed directly at integer destinations.
function pixelOffset(int $index, int $stride): int {
    return $index * $stride;
}

$width = (int) ($argv[1] ?? '320');
$row = (int) ($argv[2] ?? '12');
$column = (int) ($argv[3] ?? '7');

int $index = ($row * $width) + $column;
$index = $index + 1;

echo 'Next pixel byte offset: ', pixelOffset($index * 4, 1), "\n";
echo 'Pixel block: ', intdiv(($row * $width) + $column, 16), "\n";
