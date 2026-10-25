<?php

namespace Showcases\Doom\Graphics;

// Cycling picture sequences (nukage, water, lava, dripping walls). Each sequence is a
// run of consecutive indices; every $speed tics the run rotates by one picture.
final class Animations {
    public buffer<int> $first;
    public buffer<int> $length;
    public int $count;
    public int $speed;

    public function __construct() {
        $this->first = buffer_new<int>(32);
        $this->length = buffer_new<int>(32);
        $this->count = 0;
        $this->speed = 8;
    }

    public function add(int $first, int $last): void {
        if ($first < 0 || $last <= $first || $this->count >= 32) {
            return;
        }
        int $span = $last - $first;
        int $length = $span + 1;
        $this->first[$this->count] = $first;
        $this->length[$this->count] = $length;
        $this->count++;
    }

    // translation[i] is the picture shown in place of picture i at this tic.
    public function apply(buffer<int> $translation, int $tic): void {
        int $step = intdiv($tic, $this->speed);
        for ($s = 0; $s < $this->count; $s++) {
            int $first = $this->first[$s];
            int $length = $this->length[$s];
            for ($i = 0; $i < $length; $i++) {
                int $shifted = $step + $i;
                int $frame = $shifted % $length;
                int $picture = $first + $frame;
                int $slot = $first + $i;
                $translation[$slot] = $picture;
            }
        }
    }
}
