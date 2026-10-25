<?php

namespace Showcases\Doom\Graphics;

use Showcases\Doom\Wad\Wad;

// Floor and ceiling flats: the 64x64 lumps between F_START and F_END, read in place
// from the WAD buffer. $translation maps a flat to the one drawn this tic.
final class Flats {
    public int $count;
    public buffer<int> $offset;
    public buffer<int> $key;
    public buffer<int> $translation;
    public Animations $animations;
    public int $sky;

    public function __construct() {
        $this->count = 0;
        $this->offset = buffer_new<int>(1);
        $this->key = buffer_new<int>(1);
        $this->translation = buffer_new<int>(1);
        $this->animations = new Animations();
        $this->sky = -1;
    }

    public function load(Wad $wad): bool {
        int $start = $wad->find("F_START");
        int $end = $wad->find("F_END");
        if ($start < 0 || $end <= $start) {
            return false;
        }
        int $slots = $end - $start;
        buffer<int> $oldOffset = $this->offset;
        buffer_free($oldOffset);
        buffer<int> $oldKey = $this->key;
        buffer_free($oldKey);
        buffer<int> $oldTranslation = $this->translation;
        buffer_free($oldTranslation);
        $this->offset = buffer_new<int>($slots);
        $this->key = buffer_new<int>($slots);
        $this->translation = buffer_new<int>($slots);
        int $count = 0;
        int $firstLump = $start + 1;
        for ($lump = $firstLump; $lump < $end; $lump++) {
            if ($wad->lumpSize[$lump] !== 4096) {
                continue;
            }
            $this->offset[$count] = $wad->lumpOffset[$lump];
            $this->key[$count] = $wad->lumpKey[$lump];
            $this->translation[$count] = $count;
            $count++;
        }
        $this->count = $count;

        $this->sky = $this->find($wad->key("F_SKY1"));
        $this->addAnimation($wad, "NUKAGE1", "NUKAGE3");
        $this->addAnimation($wad, "FWATER1", "FWATER4");
        $this->addAnimation($wad, "SWATER1", "SWATER4");
        $this->addAnimation($wad, "LAVA1", "LAVA4");
        $this->addAnimation($wad, "BLOOD1", "BLOOD3");
        $this->addAnimation($wad, "RROCK05", "RROCK08");
        $this->addAnimation($wad, "SLIME01", "SLIME04");
        $this->addAnimation($wad, "SLIME05", "SLIME08");
        $this->addAnimation($wad, "SLIME09", "SLIME12");
        return $count > 0;
    }

    public function addAnimation(Wad $wad, string $firstName, string $lastName): void {
        $this->animations->add($this->find($wad->key($firstName)), $this->find($wad->key($lastName)));
    }

    public function find(int $key): int {
        for ($f = 0; $f < $this->count; $f++) {
            if ($this->key[$f] === $key) {
                return $f;
            }
        }
        return -1;
    }

    public function animate(int $tic): void {
        $this->animations->apply($this->translation, $tic);
    }
}
