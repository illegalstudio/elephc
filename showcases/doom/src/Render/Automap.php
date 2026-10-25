<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\Data\Linedef;
use Showcases\Doom\Data\Sector;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Play\Player;
use Showcases\Doom\Support\Trig;

// DOOM's automap, north up and centered on the player: lines the player has seen,
// red for walls, brown where the floor changes height, yellow where the ceiling
// does, and a white arrow for the player.
final class Automap {
    public Screen $screen;
    public int $viewHeight;
    public int $scale;

    public function __construct(Screen $screen, int $viewHeight) {
        $this->screen = $screen;
        $this->viewHeight = $viewHeight;
        // map units per screen pixel
        $this->scale = 5;
    }

    public function draw(MapData $map, Player $player, Trig $trig): void {
        Screen $screen = $this->screen;
        int $areaEnd = $this->viewHeight * $screen->width;
        buffer<u8> $pixels = $screen->pixels;
        for ($i = 0; $i < $areaEnd; $i++) {
            $pixels[$i] = 0;
        }
        int $centerX = $player->unitX();
        int $centerY = $player->unitY();
        buffer<Linedef> $lines = $map->linedefs;
        buffer<Sector> $sectors = $map->sectors;
        for ($l = 0; $l < $map->linedefCount; $l++) {
            if ($lines[$l]->mapped === 0) {
                continue;
            }
            int $hidden = $lines[$l]->flags & 128;
            if ($hidden !== 0) {
                continue;
            }
            int $front = $lines[$l]->front_sector;
            int $back = $lines[$l]->back_sector;
            int $secret = $lines[$l]->flags & 32;
            int $color = 176;
            if ($front >= 0 && $back >= 0 && $secret === 0) {
                if ($sectors[$front]->floor_height !== $sectors[$back]->floor_height) {
                    $color = 64;
                } else if ($sectors[$front]->ceiling_height !== $sectors[$back]->ceiling_height) {
                    $color = 231;
                } else {
                    continue;
                }
            }
            $this->mapLine($lines[$l]->x1, $lines[$l]->y1, $lines[$l]->x2, $lines[$l]->y2, $centerX, $centerY, $color);
        }

        // the player: an arrow pointing where they look
        int $cos = $trig->cos($player->angle);
        int $sin = $trig->sin($player->angle);
        int $noseX = $this->along($centerX, $cos, 24);
        int $noseY = $this->along($centerY, $sin, 24);
        int $tailX = $this->along($centerX, $cos, -16);
        int $tailY = $this->along($centerY, $sin, -16);
        int $leftAngle = $player->angle + 3072;
        int $rightAngle = $player->angle - 3072;
        int $leftX = $this->along($centerX, $trig->cos($leftAngle), 12);
        int $leftY = $this->along($centerY, $trig->sin($leftAngle), 12);
        int $rightX = $this->along($centerX, $trig->cos($rightAngle), 12);
        int $rightY = $this->along($centerY, $trig->sin($rightAngle), 12);
        $this->mapLine($tailX, $tailY, $noseX, $noseY, $centerX, $centerY, 209);
        $this->mapLine($noseX, $noseY, $leftX, $leftY, $centerX, $centerY, 209);
        $this->mapLine($noseX, $noseY, $rightX, $rightY, $centerX, $centerY, 209);
    }

    public function along(int $origin, int $unit, int $distance): int {
        int $scaled = $unit * $distance;
        int $offset = $scaled >> 16;
        int $point = $origin + $offset;
        return $point;
    }

    public function mapLine(int $x1, int $y1, int $x2, int $y2, int $centerX, int $centerY, int $color): void {
        int $screenX1 = $this->toScreenX($x1, $centerX);
        int $screenY1 = $this->toScreenY($y1, $centerY);
        int $screenX2 = $this->toScreenX($x2, $centerX);
        int $screenY2 = $this->toScreenY($y2, $centerY);
        // skip lines entirely outside the map area
        int $maxY = $this->viewHeight - 1;
        if (($screenX1 < 0 && $screenX2 < 0) || ($screenX1 > 319 && $screenX2 > 319)) {
            return;
        }
        if (($screenY1 < 0 && $screenY2 < 0) || ($screenY1 > $maxY && $screenY2 > $maxY)) {
            return;
        }
        $this->screen->line($screenX1, $screenY1, $screenX2, $screenY2, $color);
    }

    public function toScreenX(int $x, int $centerX): int {
        int $relative = $x - $centerX;
        int $scaled = intdiv($relative, $this->scale);
        int $screenX = 160 + $scaled;
        return $screenX;
    }

    public function toScreenY(int $y, int $centerY): int {
        int $relative = $y - $centerY;
        int $scaled = intdiv($relative, $this->scale);
        int $half = intdiv($this->viewHeight, 2);
        int $screenY = $half - $scaled;
        return $screenY;
    }
}
