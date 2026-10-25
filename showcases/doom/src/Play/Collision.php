<?php

namespace Showcases\Doom\Play;

use Showcases\Doom\Bsp\BspWalker;
use Showcases\Doom\Data\Linedef;
use Showcases\Doom\Data\Mobj;
use Showcases\Doom\Data\Sector;
use Showcases\Doom\Map\MapData;

// Where the player may stand. A move is checked against every linedef in the
// BLOCKMAP cells under the player's box: one-sided and blocking lines stop it,
// two-sided lines narrow the floor and ceiling of the opening, and the result
// must leave 56 units of room, no step over 24, and no solid object in the way.
// Positions here are whole map units.
final class Collision {
    public MapData $map;
    public BspWalker $locator;
    public Things $things;
    public buffer<int> $lineStamp;
    public int $stamp;
    public int $floorZ;
    public int $ceilingZ;
    // special lines the last checked box touched, for walk-over triggers
    public buffer<int> $touched;
    public int $touchedCount;

    public function __construct(MapData $map, BspWalker $locator, Things $things) {
        $this->map = $map;
        $this->locator = $locator;
        $this->things = $things;
        $this->lineStamp = buffer_new<int>($map->linedefCount + 1);
        $this->stamp = 0;
        $this->floorZ = 0;
        $this->ceilingZ = 0;
        $this->touched = buffer_new<int>(64);
        $this->touchedCount = 0;
    }

    public function release(): void {
        buffer<int> $lineStamp = $this->lineStamp;
        buffer_free($lineStamp);
        buffer<int> $touched = $this->touched;
        buffer_free($touched);
    }

    public function sectorAt(int $x, int $y): int {
        int $subSector = $this->locator->subSectorAt($this->map, $x, $y);
        if ($subSector < 0) {
            return -1;
        }
        return $this->map->subSectors[$subSector]->sector;
    }

    // Can a player of radius 16 stand at (x, y) with feet at $z? Sets floorZ and ceilingZ.
    public function checkPosition(int $x, int $y, int $z): bool {
        int $sector = $this->sectorAt($x, $y);
        if ($sector < 0) {
            return false;
        }
        buffer<Sector> $sectors = $this->map->sectors;
        $this->floorZ = $sectors[$sector]->floor_height;
        $this->ceilingZ = $sectors[$sector]->ceiling_height;
        $this->touchedCount = 0;
        int $left = $x - 16;
        int $right = $x + 16;
        int $bottom = $y - 16;
        int $top = $y + 16;
        if (!$this->checkThings($x, $y)) {
            return false;
        }
        if (!$this->checkLines($left, $right, $bottom, $top)) {
            return false;
        }
        int $room = $this->ceilingZ - $this->floorZ;
        int $headroom = $this->ceilingZ - $z;
        int $step = $this->floorZ - $z;
        return $room >= 56 && $headroom >= 56 && $step <= 24;
    }

    public function checkThings(int $x, int $y): bool {
        buffer<Mobj> $mobjs = $this->things->mobjs;
        for ($i = 0; $i < $this->things->count; $i++) {
            if ($mobjs[$i]->solid === 0 || $mobjs[$i]->active === 0) {
                continue;
            }
            int $reach = $mobjs[$i]->radius + 16;
            int $dx = $mobjs[$i]->x - $x;
            int $dy = $mobjs[$i]->y - $y;
            if ($dx < $reach && $dx > -$reach && $dy < $reach && $dy > -$reach) {
                return false;
            }
        }
        return true;
    }

    public function checkLines(int $left, int $right, int $bottom, int $top): bool {
        MapData $map = $this->map;
        if ($map->blockColumns <= 0) {
            return $this->checkLineRange($left, $right, $bottom, $top);
        }
        $this->stamp++;
        int $leftOffset = $left - $map->blockOriginX;
        int $rightOffset = $right - $map->blockOriginX;
        int $bottomOffset = $bottom - $map->blockOriginY;
        int $topOffset = $top - $map->blockOriginY;
        int $cellX1 = $leftOffset >> 7;
        int $cellX2 = $rightOffset >> 7;
        int $cellY1 = $bottomOffset >> 7;
        int $cellY2 = $topOffset >> 7;
        if ($cellX1 < 0) {
            $cellX1 = 0;
        }
        if ($cellY1 < 0) {
            $cellY1 = 0;
        }
        int $lastColumn = $map->blockColumns - 1;
        int $lastRow = $map->blockRows - 1;
        if ($cellX2 > $lastColumn) {
            $cellX2 = $lastColumn;
        }
        if ($cellY2 > $lastRow) {
            $cellY2 = $lastRow;
        }
        buffer<int> $cells = $map->blockCell;
        buffer<int> $list = $map->blockList;
        int $listEnd = buffer_len($list);
        int $rowEnd = $cellY2 + 1;
        int $columnEnd = $cellX2 + 1;
        for ($cy = $cellY1; $cy < $rowEnd; $cy++) {
            int $rowStart = $cy * $map->blockColumns;
            for ($cx = $cellX1; $cx < $columnEnd; $cx++) {
                int $cell = $rowStart + $cx;
                for ($at = $cells[$cell]; $at < $listEnd; $at++) {
                    int $line = $list[$at];
                    if ($line < 0) {
                        break;
                    }
                    if ($line >= $map->linedefCount || $this->lineStamp[$line] === $this->stamp) {
                        continue;
                    }
                    $this->lineStamp[$line] = $this->stamp;
                    if (!$this->checkLine($line, $left, $right, $bottom, $top)) {
                        return false;
                    }
                }
            }
        }
        return true;
    }

    // Fallback for maps without a BLOCKMAP: every line.
    public function checkLineRange(int $left, int $right, int $bottom, int $top): bool {
        for ($line = 0; $line < $this->map->linedefCount; $line++) {
            if (!$this->checkLine($line, $left, $right, $bottom, $top)) {
                return false;
            }
        }
        return true;
    }

    public function checkLine(int $line, int $left, int $right, int $bottom, int $top): bool {
        buffer<Linedef> $lines = $this->map->linedefs;
        int $x1 = $lines[$line]->x1;
        int $y1 = $lines[$line]->y1;
        int $x2 = $lines[$line]->x2;
        int $y2 = $lines[$line]->y2;
        // bounding boxes first
        if (($x1 < $left && $x2 < $left) || ($x1 > $right && $x2 > $right)) {
            return true;
        }
        if (($y1 < $bottom && $y2 < $bottom) || ($y1 > $top && $y2 > $top)) {
            return true;
        }
        // a box whose four corners are on one side of the line does not touch it
        int $dx = $x2 - $x1;
        int $dy = $y2 - $y1;
        int $s1 = $this->cornerSide($dx, $dy, $x1, $y1, $left, $bottom);
        int $s2 = $this->cornerSide($dx, $dy, $x1, $y1, $right, $bottom);
        int $s3 = $this->cornerSide($dx, $dy, $x1, $y1, $left, $top);
        int $s4 = $this->cornerSide($dx, $dy, $x1, $y1, $right, $top);
        int $sum = $s1 + $s2;
        int $sum3 = $sum + $s3;
        int $total = $sum3 + $s4;
        if ($total === 4 || $total === -4) {
            return true;
        }

        if ($lines[$line]->special !== 0 && $this->touchedCount < 64) {
            $this->touched[$this->touchedCount] = $line;
            $this->touchedCount++;
        }
        int $front = $lines[$line]->front_sector;
        int $back = $lines[$line]->back_sector;
        int $blocking = $lines[$line]->flags & 1;
        if ($front < 0 || $back < 0 || $blocking !== 0) {
            return false;
        }
        buffer<Sector> $sectors = $this->map->sectors;
        int $openTop = $sectors[$front]->ceiling_height;
        int $backCeiling = $sectors[$back]->ceiling_height;
        if ($backCeiling < $openTop) {
            $openTop = $backCeiling;
        }
        int $openBottom = $sectors[$front]->floor_height;
        int $backFloor = $sectors[$back]->floor_height;
        if ($backFloor > $openBottom) {
            $openBottom = $backFloor;
        }
        if ($openTop < $this->ceilingZ) {
            $this->ceilingZ = $openTop;
        }
        if ($openBottom > $this->floorZ) {
            $this->floorZ = $openBottom;
        }
        return true;
    }

    // -1 on the front (right) side of the line, 1 behind it, 0 on it.
    public function cornerSide(int $dx, int $dy, int $x1, int $y1, int $px, int $py): int {
        int $toX = $px - $x1;
        int $toY = $py - $y1;
        int $first = $dx * $toY;
        int $second = $dy * $toX;
        if ($first < $second) {
            return -1;
        }
        if ($first > $second) {
            return 1;
        }
        return 0;
    }

    public function pointSide(int $line, int $px, int $py): int {
        buffer<Linedef> $lines = $this->map->linedefs;
        int $x1 = $lines[$line]->x1;
        int $y1 = $lines[$line]->y1;
        int $dx = $lines[$line]->x2 - $x1;
        int $dy = $lines[$line]->y2 - $y1;
        return $this->cornerSide($dx, $dy, $x1, $y1, $px, $py);
    }

    // The nearest line a use from (x, y) along the 16.16 direction (dirX, dirY) reaches
    // within 64 units: a line with a special, or -1 when a wall or closed opening comes first.
    public function useTarget(int $x, int $y, int $dirX, int $dirY): int {
        buffer<Linedef> $lines = $this->map->linedefs;
        buffer<Sector> $sectors = $this->map->sectors;
        int $reachX = $dirX * 64;
        int $reachY = $dirY * 64;
        int $endX = $reachX >> 16;
        int $endY = $reachY >> 16;
        int $best = -1;
        int $bestT = 65537;
        for ($line = 0; $line < $this->map->linedefCount; $line++) {
            int $t = $this->traceHit($line, $x, $y, $endX, $endY);
            if ($t < 0 || $t >= $bestT) {
                continue;
            }
            int $front = $lines[$line]->front_sector;
            int $back = $lines[$line]->back_sector;
            bool $closed = $front < 0 || $back < 0;
            if (!$closed) {
                int $openTop = $sectors[$front]->ceiling_height;
                if ($sectors[$back]->ceiling_height < $openTop) {
                    $openTop = $sectors[$back]->ceiling_height;
                }
                int $openBottom = $sectors[$front]->floor_height;
                if ($sectors[$back]->floor_height > $openBottom) {
                    $openBottom = $sectors[$back]->floor_height;
                }
                $closed = $openTop <= $openBottom;
            }
            if ($lines[$line]->special === 0 && !$closed) {
                continue;
            }
            $bestT = $t;
            $best = $line;
            if ($lines[$line]->special === 0) {
                $best = -1;
            }
        }
        return $best;
    }

    // Where the trace from (x, y) by (dx, dy) crosses a line, as 0..65536 along the trace, or -1.
    public function traceHit(int $line, int $x, int $y, int $dx, int $dy): int {
        buffer<Linedef> $lines = $this->map->linedefs;
        int $x1 = $lines[$line]->x1;
        int $y1 = $lines[$line]->y1;
        int $ex = $lines[$line]->x2 - $x1;
        int $ey = $lines[$line]->y2 - $y1;
        int $denominatorA = $dx * $ey;
        int $denominatorB = $dy * $ex;
        int $denominator = $denominatorA - $denominatorB;
        if ($denominator === 0) {
            return -1;
        }
        int $fromX = $x1 - $x;
        int $fromY = $y1 - $y;
        int $alongTraceA = $fromX * $ey;
        int $alongTraceB = $fromY * $ex;
        int $alongTrace = $alongTraceA - $alongTraceB;
        int $alongLineA = $fromX * $dy;
        int $alongLineB = $fromY * $dx;
        int $alongLine = $alongLineA - $alongLineB;
        int $traceScaled = $alongTrace << 16;
        int $lineScaled = $alongLine << 16;
        int $t = intdiv($traceScaled, $denominator);
        int $u = intdiv($lineScaled, $denominator);
        if ($t < 0 || $t > 65536 || $u < 0 || $u > 65536) {
            return -1;
        }
        return $t;
    }
}
