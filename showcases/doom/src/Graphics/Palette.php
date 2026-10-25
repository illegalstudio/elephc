<?php

namespace Showcases\Doom\Graphics;

use Showcases\Doom\Wad\Wad;

// PLAYPAL's palettes as ARGB8888 (0 normal, 1-8 red pain, 9-12 gold pickup, 13 green
// radiation suit), and COLORMAP: 34 tables of 256 palette indices
// (0 = full bright ... 31 = darkest, 32 = invulnerability, 33 = black).
final class Palette {
    public buffer<u32> $argb;
    public buffer<u8> $colormaps;
    public int $count;

    public function __construct() {
        $this->argb = buffer_new<u32>(256);
        $this->colormaps = buffer_new<u8>(8704);
        $this->count = 1;
    }

    public function load(Wad $wad): bool {
        int $palette = $wad->find("PLAYPAL");
        if ($palette < 0 || $wad->lumpSize[$palette] < 768) {
            return false;
        }
        buffer<u8> $data = $wad->data;
        int $count = intdiv($wad->lumpSize[$palette], 768);
        if ($count > 14) {
            $count = 14;
        }
        int $entries = $count * 256;
        $this->count = $count;
        buffer<u32> $oldArgb = $this->argb;
        buffer_free($oldArgb);
        $this->argb = buffer_new<u32>($entries);
        buffer<u32> $argb = $this->argb;
        int $base = $wad->lumpOffset[$palette];
        for ($i = 0; $i < $entries; $i++) {
            int $entry = $i * 3;
            int $redAt = $base + $entry;
            int $greenAt = $redAt + 1;
            int $blueAt = $redAt + 2;
            int $red = $data[$redAt] << 16;
            int $green = $data[$greenAt] << 8;
            int $rg = $red | $green;
            int $rgb = $rg | $data[$blueAt];
            $argb[$i] = 4278190080 | $rgb;
        }

        buffer<u8> $colormaps = $this->colormaps;
        int $colormap = $wad->find("COLORMAP");
        if ($colormap < 0 || $wad->lumpSize[$colormap] < 8704) {
            // no light tables: every level maps a color to itself
            for ($i = 0; $i < 8704; $i++) {
                $colormaps[$i] = $i & 255;
            }
            return true;
        }
        int $source = $wad->lumpOffset[$colormap];
        for ($i = 0; $i < 8704; $i++) {
            int $at = $source + $i;
            $colormaps[$i] = $data[$at];
        }
        return true;
    }
}
