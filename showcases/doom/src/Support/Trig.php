<?php

namespace Showcases\Doom\Support;

// Fine angles as in DOOM: 8192 per turn, 0 = east, 2048 = north.
// Sine and cosine are 16.16 fixed point.
final class Trig {
    public buffer<int> $sine;
    public buffer<int> $cosine;

    public function __construct() {
        $this->sine = buffer_new<int>(8192);
        $this->cosine = buffer_new<int>(8192);
        buffer<int> $sine = $this->sine;
        buffer<int> $cosine = $this->cosine;
        for ($i = 0; $i < 8192; $i++) {
            float $radians = $i * M_PI / 4096.0;
            $sine[$i] = (int)round(sin($radians) * 65536.0);
            $cosine[$i] = (int)round(cos($radians) * 65536.0);
        }
    }

    public function sin(int $angle): int {
        int $wrapped = $angle & 8191;
        return $this->sine[$wrapped];
    }

    public function cos(int $angle): int {
        int $wrapped = $angle & 8191;
        return $this->cosine[$wrapped];
    }

    // Map things store their facing in degrees.
    public function fromDegrees(int $degrees): int {
        int $scaled = $degrees * 8192;
        int $angle = intdiv($scaled, 360);
        return $angle & 8191;
    }
}
