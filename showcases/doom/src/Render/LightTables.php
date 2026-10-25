<?php

namespace Showcases\Doom\Render;

// DOOM's light model: a sector's light level picks one of 16 bands, and distance
// darkens it further. Entries are byte offsets of a COLORMAP table (level * 256).
//   walls:  band b, scale index s = (focal / depth) * 16, capped at 47
//           level = (15 - b) * 4 - s / 2
//   planes: band b, distance index z = distance / 16, capped at 127
//           level = (15 - b) * 4 - (160 / (z + 1)) / 2
final class LightTables {
    public buffer<int> $wall;
    public buffer<int> $plane;

    public function __construct() {
        $this->wall = buffer_new<int>(768);
        $this->plane = buffer_new<int>(2048);
        buffer<int> $wall = $this->wall;
        buffer<int> $plane = $this->plane;
        for ($band = 0; $band < 16; $band++) {
            int $inverse = 15 - $band;
            int $start = $inverse * 4;
            int $wallRow = $band * 48;
            for ($s = 0; $s < 48; $s++) {
                int $fade = intdiv($s, 2);
                int $level = $start - $fade;
                int $slot = $wallRow + $s;
                $wall[$slot] = $this->offset($level);
            }
            int $planeRow = $band * 128;
            for ($z = 0; $z < 128; $z++) {
                int $steps = $z + 1;
                int $scale = intdiv(160, $steps);
                int $fade = intdiv($scale, 2);
                int $level = $start - $fade;
                int $slot = $planeRow + $z;
                $plane[$slot] = $this->offset($level);
            }
        }
    }

    public function offset(int $level): int {
        if ($level < 0) {
            return 0;
        }
        if ($level > 31) {
            return 7936;
        }
        int $offset = $level * 256;
        return $offset;
    }

    // Light band of a sector level, with DOOM's fake contrast for axis-aligned walls.
    public function band(int $lightLevel, int $contrast): int {
        int $base = $lightLevel >> 4;
        int $band = $base + $contrast;
        if ($band < 0) {
            return 0;
        }
        if ($band > 15) {
            return 15;
        }
        return $band;
    }
}
