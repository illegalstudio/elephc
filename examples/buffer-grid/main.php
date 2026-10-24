<?php

// A uniform grid that counts the points in each cell, the first step of a
// collision broadphase.
//
// Both buffer lengths are int arithmetic: `$this->columns * $this->rows` sizes
// the cell table and `2 * buffer_len($old)` grows the point list. PHP may turn
// an overflowing int result into a float, so buffer_new<T>() converts such a
// length when it allocates, and aborts unless it is a whole number in the int
// range.

packed class Point
{
    public float $x;
    public float $y;
}

final class PointGrid
{
    private buffer<Point> $points;
    private buffer<int> $counts;
    private int $count = 0;

    public function __construct(
        private int $columns,
        private int $rows,
        private float $cellSize,
    ) {
        $this->points = buffer_new<Point>(2);
        $this->counts = buffer_new<int>($this->columns * $this->rows);
    }

    public function add(float $x, float $y): void
    {
        if ($this->count === buffer_len($this->points)) {
            $old = $this->points;
            $this->points = buffer_new<Point>(2 * buffer_len($old));
            for ($i = 0; $i < $this->count; $i++) {
                $this->points[$i]->x = $old[$i]->x;
                $this->points[$i]->y = $old[$i]->y;
            }
            buffer_free($old);
        }
        $this->points[$this->count]->x = $x;
        $this->points[$this->count]->y = $y;
        $this->count++;
    }

    public function capacity(): int
    {
        return buffer_len($this->points);
    }

    public function cells(): int
    {
        return buffer_len($this->counts);
    }

    // Counts the points in each cell and returns how many cells hold any.
    public function bin(): int
    {
        for ($cell = 0; $cell < buffer_len($this->counts); $cell++) {
            $this->counts[$cell] = 0;
        }
        $used = 0;
        for ($i = 0; $i < $this->count; $i++) {
            $column = min($this->columns - 1, (int) ($this->points[$i]->x / $this->cellSize));
            $row = min($this->rows - 1, (int) ($this->points[$i]->y / $this->cellSize));
            $cell = $row * $this->columns + $column;
            if ($this->counts[$cell] === 0) {
                $used++;
            }
            $this->counts[$cell] = $this->counts[$cell] + 1;
        }
        return $used;
    }

    public function countAt(int $column, int $row): int
    {
        return $this->counts[$row * $this->columns + $column];
    }
}

$grid = new PointGrid(4, 3, 10.0);
for ($i = 0; $i < 10; $i++) {
    $grid->add($i * 3.5, $i * 2.5);
}
echo "cells=", $grid->cells(), " capacity=", $grid->capacity(), "\n";
echo "used cells=", $grid->bin(), "\n";
echo "cell (0,0)=", $grid->countAt(0, 0), " cell (1,1)=", $grid->countAt(1, 1), "\n";
