<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\Play\Player;
use Showcases\Doom\Wad\Wad;

// The status bar and on-screen text, drawn with the WAD's own pictures: STBAR with
// the big red STTNUM digits for ammo, health and armor, the STARMS weapon panel,
// the marine's face (STFST, by pain level and glance direction), key cards, ammo
// tallies in small STYSNUM digits, and STCFN font text for messages and the FPS.
final class Hud {
    public Wad $wad;
    public Screen $screen;
    public int $top;
    public int $statusBar;
    public int $arms;
    public int $percent;
    public int $faceDead;
    public buffer<int> $bigDigit;
    public buffer<int> $smallDigit;
    public buffer<int> $grayDigit;
    public buffer<int> $keyCard;
    public buffer<int> $face;
    public buffer<int> $font;
    public int $bigDigitWidth;
    public int $faceLook;
    public string $message;
    public int $messageTics;

    public function __construct(Wad $wad, Screen $screen, int $top) {
        $this->wad = $wad;
        $this->screen = $screen;
        $this->top = $top;
        $this->statusBar = $wad->find("STBAR");
        $this->arms = $wad->find("STARMS");
        $this->percent = $wad->find("STTPRCNT");
        $this->faceDead = $wad->find("STFDEAD0");
        $this->bigDigit = buffer_new<int>(10);
        $this->smallDigit = buffer_new<int>(10);
        $this->grayDigit = buffer_new<int>(10);
        for ($d = 0; $d < 10; $d++) {
            $this->bigDigit[$d] = $wad->find("STTNUM" . $d);
            $this->smallDigit[$d] = $wad->find("STYSNUM" . $d);
            $this->grayDigit[$d] = $wad->find("STGNUM" . $d);
        }
        $this->keyCard = buffer_new<int>(6);
        for ($k = 0; $k < 6; $k++) {
            $this->keyCard[$k] = $wad->find("STKEYS" . $k);
        }
        $this->face = buffer_new<int>(15);
        for ($pain = 0; $pain < 5; $pain++) {
            for ($look = 0; $look < 3; $look++) {
                int $row = $pain * 3;
                int $slot = $row + $look;
                $this->face[$slot] = $wad->find("STFST" . $pain . $look);
            }
        }
        // STCFN033 .. STCFN095 cover '!' .. '_'
        $this->font = buffer_new<int>(64);
        for ($c = 0; $c < 64; $c++) {
            int $code = $c + 32;
            string $digits = (string)$code;
            if ($code < 100) {
                $digits = "0" . $code;
            }
            $this->font[$c] = $wad->find("STCFN" . $digits);
        }
        $this->bigDigitWidth = 14;
        if ($this->bigDigit[0] >= 0) {
            $this->bigDigitWidth = $wad->i16($wad->lumpOffset[$this->bigDigit[0]]);
        }
        $this->faceLook = 1;
        $this->message = "";
        $this->messageTics = 0;
    }

    public function say(string $message): void {
        $this->message = $message;
        $this->messageTics = 140;
    }

    public function tick(int $levelTime, int $random): void {
        if ($this->messageTics > 0) {
            $this->messageTics--;
        }
        // the face glances around about twice a second
        int $phase = $levelTime % 17;
        if ($phase === 0) {
            $this->faceLook = $random % 3;
        }
    }

    public function drawStatusBar(Player $player): void {
        Screen $screen = $this->screen;
        int $top = $this->top;
        $screen->drawPatch($this->wad, $this->statusBar, 0, $top);
        int $numbersY = $top + 3;
        $this->bigNumber($player->bullets, 44, $numbersY);
        $this->bigNumber($player->health, 90, $numbersY);
        $screen->drawPatch($this->wad, $this->percent, 90, $numbersY);
        $this->bigNumber($player->armor, 221, $numbersY);
        $screen->drawPatch($this->wad, $this->percent, 221, $numbersY);

        // weapon slots 2-7: yellow when owned
        $screen->drawPatch($this->wad, $this->arms, 104, $top);
        for ($slot = 0; $slot < 6; $slot++) {
            int $weapon = $slot + 2;
            int $column = $slot % 3;
            int $row = intdiv($slot, 3);
            int $columnOffset = $column * 12;
            int $rowOffset = $row * 10;
            int $x = 111 + $columnOffset;
            int $rowTop = $top + 4;
            int $y = $rowTop + $rowOffset;
            int $bit = 1 << $weapon;
            int $owned = $player->weapons & $bit;
            if ($owned !== 0) {
                $screen->drawPatch($this->wad, $this->smallDigit[$weapon], $x, $y);
            } else {
                $screen->drawPatch($this->wad, $this->grayDigit[$weapon], $x, $y);
            }
        }

        $screen->drawPatch($this->wad, $this->faceLump($player), 143, $top);

        // key cards (blue, yellow, red) in three rows
        for ($k = 0; $k < 3; $k++) {
            int $bit = 1 << $k;
            int $owned = $player->keys & $bit;
            if ($owned === 0) {
                continue;
            }
            int $rowOffset = $k * 10;
            int $rowTop = $top + 3;
            int $y = $rowTop + $rowOffset;
            $screen->drawPatch($this->wad, $this->keyCard[$k], 239, $y);
        }

        // ammo tallies: current and maximum for bullets, shells, rockets, cells
        int $bulletsY = $top + 5;
        int $shellsY = $top + 11;
        int $rocketsY = $top + 17;
        int $cellsY = $top + 23;
        $this->smallNumber($player->bullets, 288, $bulletsY);
        $this->smallNumber(200, 314, $bulletsY);
        $this->smallNumber($player->shells, 288, $shellsY);
        $this->smallNumber(50, 314, $shellsY);
        $this->smallNumber(0, 288, $rocketsY);
        $this->smallNumber(50, 314, $rocketsY);
        $this->smallNumber(0, 288, $cellsY);
        $this->smallNumber(300, 314, $cellsY);
    }

    public function faceLump(Player $player): int {
        if ($player->health <= 0) {
            return $this->faceDead;
        }
        int $health = $player->health;
        if ($health > 100) {
            $health = 100;
        }
        int $missing = 100 - $health;
        int $scaled = $missing * 5;
        int $pain = intdiv($scaled, 101);
        int $row = $pain * 3;
        int $slot = $row + $this->faceLook;
        return $this->face[$slot];
    }

    // Right-aligned at $right, like DOOM's number widgets.
    public function bigNumber(int $value, int $right, int $y): void {
        int $remaining = $value;
        if ($remaining < 0) {
            $remaining = 0;
        }
        int $x = $right;
        for ($place = 0; $place < 3; $place++) {
            int $digit = $remaining % 10;
            int $left = $x - $this->bigDigitWidth;
            $x = $left;
            $this->screen->drawPatch($this->wad, $this->bigDigit[$digit], $x, $y);
            int $rest = intdiv($remaining, 10);
            $remaining = $rest;
            if ($remaining === 0) {
                break;
            }
        }
    }

    public function smallNumber(int $value, int $right, int $y): void {
        int $remaining = $value;
        if ($remaining < 0) {
            $remaining = 0;
        }
        int $x = $right;
        for ($place = 0; $place < 3; $place++) {
            int $digit = $remaining % 10;
            int $left = $x - 4;
            $x = $left;
            $this->screen->drawPatch($this->wad, $this->smallDigit[$digit], $x, $y);
            int $rest = intdiv($remaining, 10);
            $remaining = $rest;
            if ($remaining === 0) {
                break;
            }
        }
    }

    // Upper-case text in the STCFN font; returns the x after the last character.
    public function text(string $text, int $x, int $y): int {
        string $upper = strtoupper($text);
        int $length = strlen($upper);
        int $cursor = $x;
        for ($i = 0; $i < $length; $i++) {
            int $code = ord($upper[$i]);
            int $glyph = $code - 32;
            if ($glyph <= 0 || $glyph >= 64 || $this->font[$glyph] < 0) {
                int $spaced = $cursor + 4;
                $cursor = $spaced;
                continue;
            }
            int $lump = $this->font[$glyph];
            $this->screen->drawPatch($this->wad, $lump, $cursor, $y);
            int $advanced = $cursor + $this->wad->i16($this->wad->lumpOffset[$lump]);
            $cursor = $advanced;
        }
        return $cursor;
    }

    public function textWidth(string $text): int {
        string $upper = strtoupper($text);
        int $length = strlen($upper);
        int $width = 0;
        for ($i = 0; $i < $length; $i++) {
            int $code = ord($upper[$i]);
            int $glyph = $code - 32;
            int $advance = 4;
            if ($glyph > 0 && $glyph < 64 && $this->font[$glyph] >= 0) {
                $advance = $this->wad->i16($this->wad->lumpOffset[$this->font[$glyph]]);
            }
            int $grown = $width + $advance;
            $width = $grown;
        }
        return $width;
    }

    public function drawOverlay(string $fps): void {
        if ($this->messageTics > 0) {
            $this->text($this->message, 1, 1);
        }
        int $width = $this->textWidth($fps);
        int $x = 318 - $width;
        $this->text($fps, $x, 1);
    }
}
