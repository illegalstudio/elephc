<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\Graphics\Palette;
use Showcases\Doom\Wad\Wad;

// The 320x200 frame in palette indices, as DOOM draws it, plus its ARGB copy for SDL.
// Loops count with ++ against an int bound and keep one arithmetic operation per
// int declaration: elephc boxes a nested operation, an arithmetic argument or an
// arithmetic reassignment into a heap Mixed in case it overflows to float.
final class Screen {
    public buffer<u8> $pixels;
    public buffer<u32> $rgb;
    public int $width;
    public int $height;

    public function __construct(int $width, int $height) {
        $this->width = $width;
        $this->height = $height;
        int $count = $width * $height;
        $this->pixels = buffer_new<u8>($count);
        $this->rgb = buffer_new<u32>($count);
    }

    // Converts through one of PLAYPAL's palettes (0 normal, others tinted for pain and pickups).
    public function toRgb(Palette $palette, int $which): void {
        buffer<u8> $pixels = $this->pixels;
        buffer<u32> $rgb = $this->rgb;
        buffer<u32> $argb = $palette->argb;
        int $chosen = $which;
        if ($chosen < 0 || $chosen >= $palette->count) {
            $chosen = 0;
        }
        int $base = $chosen * 256;
        int $count = $this->width * $this->height;
        for ($i = 0; $i < $count; $i++) {
            int $entry = $base + $pixels[$i];
            $rgb[$i] = $argb[$entry];
        }
    }

    public function fill(int $color): void {
        buffer<u8> $pixels = $this->pixels;
        int $count = $this->width * $this->height;
        for ($i = 0; $i < $count; $i++) {
            $pixels[$i] = $color;
        }
    }

    public function fillRect(int $x, int $y, int $w, int $h, int $color): void {
        int $right = $x + $w;
        int $bottom = $y + $h;
        for ($row = $y; $row < $bottom; $row++) {
            int $last = $right - 1;
            $this->hline($x, $last, $row, $color);
        }
    }

    // A WAD picture (patch) at 1:1, honoring its left/top offsets and clipped to the frame.
    public function drawPatch(Wad $wad, int $lump, int $x, int $y): void {
        if ($lump < 0) {
            return;
        }
        buffer<u8> $data = $wad->data;
        buffer<u8> $pixels = $this->pixels;
        int $patchAt = $wad->lumpOffset[$lump];
        int $leftAt = $patchAt + 4;
        int $topAt = $patchAt + 6;
        int $originX = $x - $wad->i16($leftAt);
        int $originY = $y - $wad->i16($topAt);
        int $patchWidth = $wad->i16($patchAt);
        int $columnsAt = $patchAt + 8;
        int $w = $this->width;
        int $h = $this->height;
        for ($c = 0; $c < $patchWidth; $c++) {
            int $sx = $originX + $c;
            if ($sx < 0 || $sx >= $w) {
                continue;
            }
            int $columnEntry = $c * 4;
            int $columnPointer = $columnsAt + $columnEntry;
            int $post = $patchAt + $wad->i32($columnPointer);
            while (true) {
                int $topDelta = $data[$post];
                if ($topDelta === 255) {
                    break;
                }
                int $lengthAt = $post + 1;
                int $length = $data[$lengthAt];
                int $pixelsAt = $post + 3;
                int $postTop = $originY + $topDelta;
                for ($k = 0; $k < $length; $k++) {
                    int $sy = $postTop + $k;
                    if ($sy >= 0 && $sy < $h) {
                        int $source = $pixelsAt + $k;
                        int $row = $sy * $w;
                        int $target = $row + $sx;
                        $pixels[$target] = $data[$source];
                    }
                }
                int $postBytes = $length + 4;
                int $next = $post + $postBytes;
                $post = $next;
            }
        }
    }

    public function point(int $x, int $y, int $color): void {
        if ($x >= 0 && $x < $this->width && $y >= 0 && $y < $this->height) {
            int $row = $y * $this->width;
            int $index = $row + $x;
            $this->pixels[$index] = $color;
        }
    }

    public function hline(int $x1, int $x2, int $y, int $color): void {
        if ($y < 0 || $y >= $this->height) {
            return;
        }
        int $left = $x1;
        int $right = $x2;
        if ($left > $right) {
            $left = $x2;
            $right = $x1;
        }
        if ($left < 0) {
            $left = 0;
        }
        int $lastColumn = $this->width - 1;
        if ($right > $lastColumn) {
            $right = $lastColumn;
        }
        buffer<u8> $pixels = $this->pixels;
        int $row = $y * $this->width;
        int $end = $right + 1;
        for ($x = $left; $x < $end; $x++) {
            int $index = $row + $x;
            $pixels[$index] = $color;
        }
    }

    // Inclusive line in 16.16 fixed point: each step derives its position from the counter.
    public function line(int $x0, int $y0, int $x1, int $y1, int $color): void {
        int $dx = $x1 - $x0;
        int $dy = $y1 - $y0;
        int $adx = $dx;
        if ($dx < 0) {
            $adx = -$dx;
        }
        int $ady = $dy;
        if ($dy < 0) {
            $ady = -$dy;
        }
        int $steps = $adx;
        if ($ady > $steps) {
            $steps = $ady;
        }
        if ($steps === 0) {
            $this->point($x0, $y0, $color);
            return;
        }
        int $fixedDx = $dx * 65536;
        int $fixedDy = $dy * 65536;
        int $stepX = intdiv($fixedDx, $steps);
        int $stepY = intdiv($fixedDy, $steps);
        int $fixedX0 = $x0 * 65536;
        int $fixedY0 = $y0 * 65536;
        int $originX = $fixedX0 + 32768;
        int $originY = $fixedY0 + 32768;
        buffer<u8> $pixels = $this->pixels;
        int $w = $this->width;
        int $h = $this->height;
        int $end = $steps + 1;
        for ($i = 0; $i < $end; $i++) {
            int $travelX = $i * $stepX;
            int $travelY = $i * $stepY;
            int $fixedX = $originX + $travelX;
            int $fixedY = $originY + $travelY;
            int $x = $fixedX >> 16;
            int $y = $fixedY >> 16;
            if ($x >= 0 && $x < $w && $y >= 0 && $y < $h) {
                int $row = $y * $w;
                int $index = $row + $x;
                $pixels[$index] = $color;
            }
        }
    }
}
