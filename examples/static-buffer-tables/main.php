<?php

// Engine-wide lookup tables and scratch space kept in static buffer<T> properties. The
// buffers are allocated once and every caller reaches the same ones through the class, with
// element access as direct as through a local buffer.

packed class Vec2
{
    public float $x;
    public float $y;
}

final class Tables
{
    public static buffer<float> $squares;
    public static buffer<Vec2> $ring;
    public static buffer<int> $scratch;
    public static ptr<Vec2> $cursor = null;

    public static function init(int $size): void
    {
        self::$squares = buffer_new<float>($size);
        self::$ring = buffer_new<Vec2>($size);
        self::$scratch = buffer_new<int>($size);
        for ($i = 0; $i < $size; $i++) {
            self::$squares[$i] = $i * $i * 1.0;
            self::$ring[$i]->x = $i * 0.5;
            self::$ring[$i]->y = $size - $i * 0.5;
        }
    }

    public static function square(int $i): float
    {
        return static::$squares[$i];
    }
}

function sumSquares(buffer<float> $table): float
{
    $sum = 0.0;
    for ($i = 0; $i < buffer_len($table); $i++) {
        $sum += $table[$i];
    }
    return $sum;
}

Tables::init(8);
echo "square(5)=", Tables::square(5), "\n";
echo "sum of squares=", sumSquares(Tables::$squares), "\n";
echo "ring[3]=", Tables::$ring[3]->x, ",", Tables::$ring[3]->y, "\n";

// Packed fields and whole elements are written in place through the static slot.
Tables::$ring[0]->x += 10.0;
echo "ring[0].x=", Tables::$ring[0]->x, "\n";
for ($i = 0; $i < buffer_len(Tables::$scratch); $i++) {
    Tables::$scratch[$i] = $i % 3;
    Tables::$scratch[$i] += 1;
}
$scratchSum = 0;
for ($i = 0; $i < buffer_len(Tables::$scratch); $i++) {
    $scratchSum += Tables::$scratch[$i];
}
echo "scratch sum=", $scratchSum, "\n";

// A static ptr<T> starts as the null pointer and can point at one record of a static buffer.
if (ptr_is_null(Tables::$cursor)) {
    Tables::$cursor = Tables::$ring[5];
}
Tables::$cursor->y = 1.0;
echo "cursor y=", Tables::$ring[5]->y, "\n";

// buffer_free() takes a local, so the static buffers are released through copies.
$squares = Tables::$squares;
$ring = Tables::$ring;
$scratch = Tables::$scratch;
buffer_free($squares);
buffer_free($ring);
buffer_free($scratch);
