<?php

namespace Showcases\Doom\Tests\Fixture;

// A small WAD the tests write to a temporary file, so they run without DOOM1.WAD.
// Its light tables map every palette index to itself, so a test reads the colors
// it drew straight from the 8-bit frame.
//
// E1M1 and E1M2 are the same map: two 256x256 rooms side by side, the west room
// (sector 0) and the east room (sector 1, tag 1), joined along x = 256 by a
// two-sided, lower-unpegged line whose middle texture is a grate with 8-unit
// holes. Things, in spawn order: a 64x64 health bonus in the east room, a blue key
// card in the west room and a hanging body in the east room; the player starts in
// the west room facing east, at the grate.
final class TestWad {
    // palette indices of the pictures
    public const WALL = 100;
    public const GRATE = 200;
    public const FLOOR = 50;
    public const CEILING = 60;
    public const BONUS = 150;
    public const KEY = 160;
    public const BODY = 170;

    // linedef of the grate, spawn index of each thing
    public const GRATE_LINE = 6;
    public const BONUS_OBJECT = 0;
    public const KEY_OBJECT = 1;
    public const BODY_OBJECT = 2;

    public array $names;
    public array $lumps;

    public function __construct() {
        $this->names = [];
        $this->lumps = [];
        $this->add("PLAYPAL", $this->palette());
        $this->add("COLORMAP", $this->colormap());
        $this->add("PNAMES", $this->i32(2) . $this->name("WALLP") . $this->name("GRATEP"));
        $this->add("TEXTURE1", $this->textures());
        $this->add("WALLP", $this->patch(64, 64, 0, 0, self::WALL, 0));
        $this->add("GRATEP", $this->patch(64, 64, 0, 0, self::GRATE, 8));
        $this->add("F_START", "");
        $this->add("FLOOR0", str_repeat(chr(self::FLOOR), 4096));
        $this->add("CEIL0", str_repeat(chr(self::CEILING), 4096));
        $this->add("F_SKY1", str_repeat(chr(1), 4096));
        $this->add("F_END", "");
        $this->add("S_START", "");
        $this->add("BON1A0", $this->patch(64, 64, 32, 64, self::BONUS, 0));
        $this->add("BKEYA0", $this->patch(16, 16, 8, 16, self::KEY, 0));
        $this->add("GOR3A0", $this->patch(16, 52, 8, 52, self::BODY, 0));
        $this->add("S_END", "");
        $this->addMap("E1M1");
        $this->addMap("E1M2");
    }

    // Writes the WAD to a new temporary file and returns its path.
    public function write(): string {
        string $path = (string)tempnam(sys_get_temp_dir(), "doomtest");
        int $count = count($this->names);
        string $body = "";
        string $directory = "";
        for ($i = 0; $i < $count; $i++) {
            string $data = (string)$this->lumps[$i];
            int $offset = 12 + strlen($body);
            $directory .= $this->i32($offset) . $this->i32(strlen($data)) . $this->name((string)$this->names[$i]);
            $body .= $data;
        }
        int $directoryAt = 12 + strlen($body);
        file_put_contents($path, "PWAD" . $this->i32($count) . $this->i32($directoryAt) . $body . $directory);
        return $path;
    }

    public function add(string $name, string $data): void {
        $this->names[] = $name;
        $this->lumps[] = $data;
    }

    public function addMap(string $marker): void {
        $this->add($marker, "");
        $this->add("THINGS", $this->things());
        $this->add("LINEDEFS", $this->linedefs());
        $this->add("SIDEDEFS", $this->sidedefs());
        $this->add("VERTEXES", $this->vertexes());
        $this->add("SEGS", $this->segs());
        $this->add("SSECTORS", $this->u16(4) . $this->u16(0) . $this->u16(4) . $this->u16(4));
        $this->add("NODES", $this->nodes());
        $this->add("SECTORS", $this->sector(0, 128, 0) . $this->sector(0, 128, 1));
        $this->add("REJECT", "");
        $this->add("BLOCKMAP", $this->blockmap());
    }

    public function palette(): string {
        string $data = "";
        for ($i = 0; $i < 256; $i++) {
            $data .= chr($i) . chr($i) . chr($i);
        }
        return $data;
    }

    // 34 identical light tables that keep every index
    public function colormap(): string {
        string $table = "";
        for ($i = 0; $i < 256; $i++) {
            $table .= chr($i);
        }
        return str_repeat($table, 34);
    }

    public function textures(): string {
        string $wall = $this->name("WALL") . $this->i32(0) . $this->u16(64) . $this->u16(64) . $this->i32(0)
            . $this->u16(1) . $this->u16(0) . $this->u16(0) . $this->u16(0) . $this->u16(1) . $this->u16(0);
        string $grate = $this->name("GRATE") . $this->i32(0) . $this->u16(64) . $this->u16(64) . $this->i32(0)
            . $this->u16(1) . $this->u16(0) . $this->u16(0) . $this->u16(1) . $this->u16(1) . $this->u16(0);
        int $first = 12;
        int $second = $first + strlen($wall);
        return $this->i32(2) . $this->i32($first) . $this->i32($second) . $wall . $grate;
    }

    // A picture of one color. With $stripe > 0, every other run of $stripe columns is empty.
    public function patch(int $width, int $height, int $left, int $top, int $color, int $stripe): string {
        string $offsets = "";
        string $columns = "";
        int $dataStart = 8 + $width * 4;
        for ($c = 0; $c < $width; $c++) {
            $offsets .= $this->i32($dataStart + strlen($columns));
            bool $hole = $stripe > 0 && intdiv($c, $stripe) % 2 === 1;
            if (!$hole) {
                $columns .= chr(0) . chr($height) . chr(0) . str_repeat(chr($color), $height) . chr(0);
            }
            $columns .= chr(255);
        }
        return $this->u16($width) . $this->u16($height) . $this->u16($left) . $this->u16($top) . $offsets . $columns;
    }

    // player start, health bonus, blue key card, hanging body (all skills)
    public function things(): string {
        return $this->thing(64, 128, 0, 1)
            . $this->thing(384, 128, 0, 2014)
            . $this->thing(128, 64, 0, 5)
            . $this->thing(448, 200, 0, 61);
    }

    public function thing(int $x, int $y, int $angle, int $type): string {
        return $this->u16($x) . $this->u16($y) . $this->u16($angle) . $this->u16($type) . $this->u16(7);
    }

    // v0 (0,0)  v1 (256,0)  v2 (512,0)  v3 (512,256)  v4 (256,256)  v5 (0,256)
    public function vertexes(): string {
        return $this->u16(0) . $this->u16(0)
            . $this->u16(256) . $this->u16(0)
            . $this->u16(512) . $this->u16(0)
            . $this->u16(512) . $this->u16(256)
            . $this->u16(256) . $this->u16(256)
            . $this->u16(0) . $this->u16(256);
    }

    // walls run with the room on their right; line 6 is the grate, east room in front
    public function linedefs(): string {
        return $this->linedef(1, 0, 1, 0, -1)
            . $this->linedef(0, 5, 1, 1, -1)
            . $this->linedef(5, 4, 1, 2, -1)
            . $this->linedef(2, 1, 1, 3, -1)
            . $this->linedef(3, 2, 1, 4, -1)
            . $this->linedef(4, 3, 1, 5, -1)
            . $this->linedef(1, 4, 20, 6, 7);
    }

    public function linedef(int $start, int $end, int $flags, int $right, int $left): string {
        return $this->u16($start) . $this->u16($end) . $this->u16($flags) . $this->u16(0) . $this->u16(0)
            . $this->u16($right) . $this->u16($left);
    }

    public function sidedefs(): string {
        return $this->sidedef("WALL", 0) . $this->sidedef("WALL", 0) . $this->sidedef("WALL", 0)
            . $this->sidedef("WALL", 1) . $this->sidedef("WALL", 1) . $this->sidedef("WALL", 1)
            . $this->sidedef("GRATE", 1) . $this->sidedef("GRATE", 0);
    }

    public function sidedef(string $middle, int $sector): string {
        return $this->u16(0) . $this->u16(0) . $this->name("-") . $this->name("-") . $this->name($middle) . $this->u16($sector);
    }

    // subsector 0 is the east room (segs 0-3), subsector 1 the west room (segs 4-7)
    public function segs(): string {
        return $this->seg(2, 1, 32768, 3, 0)
            . $this->seg(3, 2, 49152, 4, 0)
            . $this->seg(4, 3, 0, 5, 0)
            . $this->seg(1, 4, 16384, 6, 0)
            . $this->seg(1, 0, 32768, 0, 0)
            . $this->seg(0, 5, 16384, 1, 0)
            . $this->seg(5, 4, 0, 2, 0)
            . $this->seg(4, 1, 49152, 6, 1);
    }

    public function seg(int $start, int $end, int $angle, int $line, int $direction): string {
        return $this->u16($start) . $this->u16($end) . $this->u16($angle) . $this->u16($line) . $this->u16($direction) . $this->u16(0);
    }

    // one partition along the grate: the east room on its right
    public function nodes(): string {
        return $this->u16(256) . $this->u16(0) . $this->u16(0) . $this->u16(256)
            . $this->u16(256) . $this->u16(0) . $this->u16(256) . $this->u16(512)
            . $this->u16(256) . $this->u16(0) . $this->u16(0) . $this->u16(256)
            . $this->u16(32768) . $this->u16(32769);
    }

    public function sector(int $floor, int $ceiling, int $tag): string {
        return $this->u16($floor) . $this->u16($ceiling) . $this->name("FLOOR0") . $this->name("CEIL0")
            . $this->u16(160) . $this->u16(0) . $this->u16($tag);
    }

    // 5 x 3 cells of 128 units from (-8, -8), every cell listing every line
    public function blockmap(): string {
        string $words = $this->u16(65528) . $this->u16(65528) . $this->u16(5) . $this->u16(3);
        int $listAt = 4 + 15;
        for ($c = 0; $c < 15; $c++) {
            $words .= $this->u16($listAt);
        }
        $words .= $this->u16(0);
        for ($l = 0; $l < 7; $l++) {
            $words .= $this->u16($l);
        }
        return $words . $this->u16(65535);
    }

    public function name(string $name): string {
        return $name . str_repeat(chr(0), 8 - strlen($name));
    }

    public function u16(int $value): string {
        int $low = $value & 255;
        int $shifted = $value >> 8;
        int $high = $shifted & 255;
        return chr($low) . chr($high);
    }

    public function i32(int $value): string {
        int $low = $value & 65535;
        int $shifted = $value >> 16;
        int $high = $shifted & 65535;
        return $this->u16($low) . $this->u16($high);
    }
}
