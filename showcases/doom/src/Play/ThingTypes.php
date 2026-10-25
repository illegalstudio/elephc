<?php

namespace Showcases\Doom\Play;

// What each DOOM thing type looks like and how big it is: sprite name, first frame
// letter, frames in its idle cycle and tics per frame, full-bright flag, radius,
// height, whether it blocks movement, and whether it hangs from the ceiling.
// Types missing here (player and deathmatch starts, teleport targets) are not drawn.
final class ThingTypes {
    public int $count;
    public buffer<int> $type;
    public array $sprite;
    public buffer<int> $firstFrame;
    public buffer<int> $frames;
    public buffer<int> $tics;
    public buffer<int> $bright;
    public buffer<int> $radius;
    public buffer<int> $height;
    public buffer<int> $solid;
    public buffer<int> $hanging;

    public function __construct() {
        $rows = [
            // monsters
            [3004, "POSS", "A", 2, 10, 0, 20, 56, 1, 0],
            [9, "SPOS", "A", 2, 10, 0, 20, 56, 1, 0],
            [3001, "TROO", "A", 2, 10, 0, 20, 56, 1, 0],
            [3002, "SARG", "A", 2, 10, 0, 30, 56, 1, 0],
            [58, "SARG", "A", 2, 10, 0, 30, 56, 1, 0],
            [3006, "SKUL", "A", 2, 10, 1, 16, 56, 1, 0],
            [3005, "HEAD", "A", 1, 10, 0, 31, 56, 1, 0],
            [3003, "BOSS", "A", 2, 10, 0, 24, 64, 1, 0],
            [16, "CYBR", "A", 2, 10, 0, 40, 110, 1, 0],
            [7, "SPID", "A", 2, 10, 0, 128, 100, 1, 0],
            // weapons and ammo
            [2001, "SHOT", "A", 1, 0, 0, 20, 16, 0, 0],
            [2002, "MGUN", "A", 1, 0, 0, 20, 16, 0, 0],
            [2003, "LAUN", "A", 1, 0, 0, 20, 16, 0, 0],
            [2004, "PLAS", "A", 1, 0, 0, 20, 16, 0, 0],
            [2005, "CSAW", "A", 1, 0, 0, 20, 16, 0, 0],
            [2006, "BFUG", "A", 1, 0, 0, 20, 16, 0, 0],
            [2007, "CLIP", "A", 1, 0, 0, 20, 16, 0, 0],
            [2008, "SHEL", "A", 1, 0, 0, 20, 16, 0, 0],
            [2010, "ROCK", "A", 1, 0, 0, 20, 16, 0, 0],
            [2047, "CELL", "A", 1, 0, 0, 20, 16, 0, 0],
            [2048, "AMMO", "A", 1, 0, 0, 20, 16, 0, 0],
            [2049, "SBOX", "A", 1, 0, 0, 20, 16, 0, 0],
            [2046, "BROK", "A", 1, 0, 0, 20, 16, 0, 0],
            [17, "CELP", "A", 1, 0, 0, 20, 16, 0, 0],
            [8, "BPAK", "A", 1, 0, 0, 20, 16, 0, 0],
            // health, armor and power-ups
            [2011, "STIM", "A", 1, 0, 0, 20, 16, 0, 0],
            [2012, "MEDI", "A", 1, 0, 0, 20, 16, 0, 0],
            [2014, "BON1", "A", 4, 6, 0, 20, 16, 0, 0],
            [2015, "BON2", "A", 4, 6, 0, 20, 16, 0, 0],
            [2018, "ARM1", "A", 2, 7, 0, 20, 16, 0, 0],
            [2019, "ARM2", "A", 2, 7, 0, 20, 16, 0, 0],
            [2013, "SOUL", "A", 4, 6, 1, 20, 16, 0, 0],
            [2022, "PINV", "A", 4, 6, 1, 20, 16, 0, 0],
            [2023, "PSTR", "A", 1, 0, 1, 20, 16, 0, 0],
            [2024, "PINS", "A", 4, 6, 1, 20, 16, 0, 0],
            [2025, "SUIT", "A", 1, 0, 1, 20, 16, 0, 0],
            [2026, "PMAP", "A", 4, 6, 1, 20, 16, 0, 0],
            [2045, "PVIS", "A", 2, 6, 1, 20, 16, 0, 0],
            [5, "BKEY", "A", 2, 10, 0, 20, 16, 0, 0],
            [6, "YKEY", "A", 2, 10, 0, 20, 16, 0, 0],
            [13, "RKEY", "A", 2, 10, 0, 20, 16, 0, 0],
            [38, "RSKU", "A", 2, 10, 0, 20, 16, 0, 0],
            [39, "YSKU", "A", 2, 10, 0, 20, 16, 0, 0],
            [40, "BSKU", "A", 2, 10, 0, 20, 16, 0, 0],
            // obstacles and decorations
            [2035, "BAR1", "A", 2, 6, 0, 10, 42, 1, 0],
            [70, "FCAN", "A", 3, 4, 1, 10, 42, 1, 0],
            [2028, "COLU", "A", 1, 0, 1, 16, 48, 1, 0],
            [48, "ELEC", "A", 1, 0, 0, 16, 128, 1, 0],
            [30, "COL1", "A", 1, 0, 0, 16, 52, 1, 0],
            [31, "COL2", "A", 1, 0, 0, 16, 52, 1, 0],
            [32, "COL3", "A", 1, 0, 0, 16, 52, 1, 0],
            [33, "COL4", "A", 1, 0, 0, 16, 52, 1, 0],
            [36, "COL5", "A", 2, 14, 0, 16, 52, 1, 0],
            [37, "COL6", "A", 1, 0, 0, 16, 52, 1, 0],
            [41, "CEYE", "A", 3, 6, 1, 16, 54, 1, 0],
            [42, "FSKU", "A", 3, 6, 1, 16, 26, 1, 0],
            [43, "TRE1", "A", 1, 0, 0, 16, 56, 1, 0],
            [54, "TRE2", "A", 1, 0, 0, 32, 108, 1, 0],
            [44, "TBLU", "A", 4, 4, 1, 16, 64, 1, 0],
            [45, "TGRN", "A", 4, 4, 1, 16, 64, 1, 0],
            [46, "TRED", "A", 4, 4, 1, 16, 64, 1, 0],
            [55, "SMBT", "A", 4, 4, 1, 16, 64, 1, 0],
            [56, "SMGT", "A", 4, 4, 1, 16, 64, 1, 0],
            [57, "SMRT", "A", 4, 4, 1, 16, 64, 1, 0],
            [34, "CAND", "A", 1, 0, 1, 20, 16, 0, 0],
            [35, "CBRA", "A", 1, 0, 1, 16, 60, 1, 0],
            [47, "SMIT", "A", 1, 0, 0, 16, 56, 1, 0],
            // corpses, gore and hanging bodies
            [10, "PLAY", "W", 1, 0, 0, 20, 16, 0, 0],
            [12, "PLAY", "W", 1, 0, 0, 20, 16, 0, 0],
            [15, "PLAY", "N", 1, 0, 0, 20, 16, 0, 0],
            [18, "POSS", "L", 1, 0, 0, 20, 16, 0, 0],
            [19, "SPOS", "L", 1, 0, 0, 20, 16, 0, 0],
            [20, "TROO", "M", 1, 0, 0, 20, 16, 0, 0],
            [21, "SARG", "N", 1, 0, 0, 20, 16, 0, 0],
            [22, "HEAD", "L", 1, 0, 0, 20, 16, 0, 0],
            [24, "POL5", "A", 1, 0, 0, 20, 16, 0, 0],
            [25, "POL1", "A", 1, 0, 0, 16, 56, 1, 0],
            [26, "POL6", "A", 2, 8, 0, 16, 56, 1, 0],
            [27, "POL4", "A", 1, 0, 0, 16, 56, 1, 0],
            [28, "POL2", "A", 1, 0, 0, 16, 56, 1, 0],
            [29, "POL3", "A", 2, 6, 1, 16, 56, 1, 0],
            [49, "GOR1", "A", 3, 10, 0, 16, 68, 1, 1],
            [50, "GOR2", "A", 1, 0, 0, 16, 84, 1, 1],
            [51, "GOR3", "A", 1, 0, 0, 16, 84, 1, 1],
            [52, "GOR4", "A", 1, 0, 0, 16, 68, 1, 1],
            [53, "GOR5", "A", 1, 0, 0, 16, 52, 1, 1],
            [59, "GOR2", "A", 1, 0, 0, 20, 84, 0, 1],
            [60, "GOR4", "A", 1, 0, 0, 20, 68, 0, 1],
            [61, "GOR3", "A", 1, 0, 0, 20, 52, 0, 1],
            [62, "GOR5", "A", 1, 0, 0, 20, 52, 0, 1],
            [63, "GOR1", "A", 3, 10, 0, 20, 68, 0, 1],
        ];
        int $count = count($rows);
        $this->count = $count;
        $this->type = buffer_new<int>($count);
        $this->firstFrame = buffer_new<int>($count);
        $this->frames = buffer_new<int>($count);
        $this->tics = buffer_new<int>($count);
        $this->bright = buffer_new<int>($count);
        $this->radius = buffer_new<int>($count);
        $this->height = buffer_new<int>($count);
        $this->solid = buffer_new<int>($count);
        $this->hanging = buffer_new<int>($count);
        $sprites = [];
        for ($i = 0; $i < $count; $i++) {
            $row = $rows[$i];
            $this->type[$i] = (int)$row[0];
            $sprites[] = (string)$row[1];
            $this->firstFrame[$i] = ord((string)$row[2]) - 65;
            $this->frames[$i] = (int)$row[3];
            $this->tics[$i] = (int)$row[4];
            $this->bright[$i] = (int)$row[5];
            $this->radius[$i] = (int)$row[6];
            $this->height[$i] = (int)$row[7];
            $this->solid[$i] = (int)$row[8];
            $this->hanging[$i] = (int)$row[9];
        }
        $this->sprite = $sprites;
    }

    public function find(int $type): int {
        for ($i = 0; $i < $this->count; $i++) {
            if ($this->type[$i] === $type) {
                return $i;
            }
        }
        return -1;
    }

    public function spriteName(int $kind): string {
        return (string)$this->sprite[$kind];
    }
}
