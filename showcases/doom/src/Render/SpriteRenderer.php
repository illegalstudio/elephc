<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\Data\Mobj;
use Showcases\Doom\Data\Seg;
use Showcases\Doom\Graphics\Sprites;
use Showcases\Doom\Play\Things;
use Showcases\Doom\Wad\Wad;

// Map objects as scaled WAD pictures, drawn far to near after the walls. Each
// sprite column is clipped by the farthest recorded seg that is still in front of
// the sprite: its saved opening already includes everything nearer, so the sprite
// shows exactly where the walls left it visible. A see-through middle texture
// behind the sprite is painted first, over the sprite's columns, so the sprite
// covers it; the ones in front are painted after all sprites.
final class SpriteRenderer {
    public BspRenderer $world;
    public Sprites $sprites;
    public buffer<u8> $wadData;
    public buffer<int> $lumpOffset;
    public Wad $wad;
    public buffer<int> $visibleIndex;
    public buffer<int> $visibleDepth;
    public buffer<int> $visibleSide;
    public buffer<int> $columnTop;
    public buffer<int> $columnBottom;

    public function __construct(BspRenderer $world, Sprites $sprites, Wad $wad) {
        $this->world = $world;
        $this->sprites = $sprites;
        $this->wad = $wad;
        $this->wadData = $wad->data;
        $this->lumpOffset = $wad->lumpOffset;
        $this->visibleIndex = buffer_new<int>(1);
        $this->visibleDepth = buffer_new<int>(1);
        $this->visibleSide = buffer_new<int>(1);
        $this->columnTop = buffer_new<int>($world->width);
        $this->columnBottom = buffer_new<int>($world->width);
    }

    public function render(Things $things, int $tic): void {
        if ($things->count <= 0) {
            return;
        }
        if (buffer_len($this->visibleIndex) < $things->count) {
            buffer<int> $oldIndex = $this->visibleIndex;
            buffer_free($oldIndex);
            buffer<int> $oldDepth = $this->visibleDepth;
            buffer_free($oldDepth);
            buffer<int> $oldSide = $this->visibleSide;
            buffer_free($oldSide);
            $this->visibleIndex = buffer_new<int>($things->count);
            $this->visibleDepth = buffer_new<int>($things->count);
            $this->visibleSide = buffer_new<int>($things->count);
        }
        BspRenderer $world = $this->world;
        buffer<Mobj> $mobjs = $things->mobjs;
        buffer<int> $order = $this->visibleIndex;
        buffer<int> $depths = $this->visibleDepth;
        buffer<int> $sides = $this->visibleSide;
        int $near = $world->nearDepth;
        int $visible = 0;
        for ($i = 0; $i < $things->count; $i++) {
            if ($mobjs[$i]->active === 0) {
                continue;
            }
            int $relX = $mobjs[$i]->x - $world->viewX;
            int $relY = $mobjs[$i]->y - $world->viewY;
            int $depth = $world->depthOf($relX, $relY);
            if ($depth < $near) {
                continue;
            }
            int $side = $world->sideOf($relX, $relY);
            // anything wider than twice its depth is well outside the 90-degree view
            int $limit = $depth * 2;
            int $reach = $side;
            if ($side < 0) {
                $reach = -$side;
            }
            int $margin = 8388608;
            int $farOut = $reach - $margin;
            if ($farOut > $limit) {
                continue;
            }
            // insert by depth, farthest first
            int $slot = $visible;
            while ($slot > 0) {
                int $previous = $slot - 1;
                if ($depths[$previous] >= $depth) {
                    break;
                }
                $depths[$slot] = $depths[$previous];
                $order[$slot] = $order[$previous];
                $sides[$slot] = $sides[$previous];
                $slot--;
            }
            $depths[$slot] = $depth;
            $order[$slot] = $i;
            $sides[$slot] = $side;
            $visible++;
        }

        for ($k = 0; $k < $visible; $k++) {
            int $index = $order[$k];
            int $definition = $things->definitionAt($index, $tic);
            $this->drawThing($mobjs, $index, $definition, $depths[$k], $sides[$k]);
        }
    }

    public function drawThing(buffer<Mobj> $mobjs, int $index, int $definition, int $depth, int $side): void {
        BspRenderer $world = $this->world;
        int $rotationSlot = $definition * 8;
        if ($this->sprites->rotated[$definition] !== 0) {
            // which of the eight views faces the camera
            int $towardX = $mobjs[$index]->x - $world->viewX;
            int $towardY = $mobjs[$index]->y - $world->viewY;
            float $radians = atan2($towardY, $towardX);
            int $viewAngle = (int)round($radians * 8192.0 / (2.0 * M_PI));
            int $relative = $viewAngle - $mobjs[$index]->angle;
            int $shifted = $relative + 4608;
            int $wrapped = $shifted & 8191;
            int $rotation = $wrapped >> 10;
            int $chosen = $rotationSlot + $rotation;
            $rotationSlot = $chosen;
        }
        int $lump = $this->sprites->lump[$rotationSlot];
        if ($lump < 0) {
            return;
        }
        bool $flip = $this->sprites->flip[$rotationSlot] !== 0;

        int $patchAt = $this->lumpOffset[$lump];
        int $topAt = $patchAt + 6;
        int $leftAt = $patchAt + 4;
        int $patchWidth = $this->wad->i16($patchAt);
        int $leftOffset = $this->wad->i16($leftAt);
        int $topOffset = $this->wad->i16($topAt);

        // horizontal extent: the picture's left offset is its origin
        int $leftShift = $leftOffset << 16;
        int $leftSide = $side - $leftShift;
        int $widthFixed = $patchWidth << 16;
        int $rightSide = $leftSide + $widthFixed;
        int $leftScreen = $world->screenX($leftSide, $depth);
        int $rightScreen = $world->screenX($rightSide, $depth);
        int $x1 = $leftScreen >> 16;
        int $rightPixel = $rightScreen >> 16;
        int $x2 = $rightPixel - 1;
        int $lastColumn = $world->width - 1;
        if ($x1 > $lastColumn || $x2 < 0 || $x1 > $x2) {
            return;
        }

        int $scale = intdiv($world->focalScale, $depth);
        int $iscale = intdiv($depth, $world->focal);
        int $columnStep = $iscale;
        int $startFrac = 0;
        if ($flip) {
            $columnStep = -$iscale;
            int $lastTexel = $widthFixed - 1;
            $startFrac = $lastTexel;
        }
        int $drawX1 = $x1;
        if ($drawX1 < 0) {
            $drawX1 = 0;
        }
        int $drawX2 = $x2;
        if ($drawX2 > $lastColumn) {
            $drawX2 = $lastColumn;
        }

        // light: the sector's band dimmed by distance, unless the frame glows
        int $colormap = 0;
        if ($mobjs[$index]->bright === 0) {
            int $lightLevel = $world->sectors[$mobjs[$index]->sector]->light_level;
            int $band = $world->lights->band($lightLevel, 0);
            int $scaleIndex = $scale >> 12;
            if ($scaleIndex > 47) {
                $scaleIndex = 47;
            }
            int $bandRow = $band * 48;
            int $lightSlot = $bandRow + $scaleIndex;
            $colormap = $world->wallLight[$lightSlot];
        }

        int $spriteTop = $mobjs[$index]->z + $topOffset;
        int $topHeight = $spriteTop - $world->viewZ;
        int $textureMid = $topHeight << 16;
        $this->clipColumns($mobjs[$index]->x, $mobjs[$index]->y, $drawX1, $drawX2, $depth);

        buffer<u8> $data = $this->wadData;
        buffer<u8> $screen = $world->screen;
        buffer<u8> $colormaps = $world->colormaps;
        buffer<int> $columnTop = $this->columnTop;
        buffer<int> $columnBottom = $this->columnBottom;
        int $centerY = $world->centerY;
        int $w = $world->width;
        int $columnsAt = $patchAt + 8;
        int $endX = $drawX2 + 1;
        for ($x = $drawX1; $x < $endX; $x++) {
            int $clipTop = $columnTop[$x];
            int $clipBottom = $columnBottom[$x];
            if ($clipTop > $clipBottom) {
                continue;
            }
            int $fromLeft = $x - $x1;
            int $travel = $fromLeft * $columnStep;
            int $frac = $startFrac + $travel;
            int $texelColumn = $frac >> 16;
            if ($texelColumn < 0 || $texelColumn >= $patchWidth) {
                continue;
            }
            int $columnEntry = $texelColumn * 4;
            int $columnPointer = $columnsAt + $columnEntry;
            int $post = $patchAt + $this->wad->i32($columnPointer);
            while (true) {
                int $topDelta = $data[$post];
                if ($topDelta === 255) {
                    break;
                }
                int $lengthAt = $post + 1;
                int $length = $data[$lengthAt];
                int $pixelsAt = $post + 3;
                // the post covers picture rows topDelta .. topDelta + length - 1
                int $postTop = $topDelta << 16;
                int $postMid = $textureMid - $postTop;
                int $topRise = $postMid * $scale;
                int $topShift = $topRise >> 16;
                int $topFixed = $world->centerYFixed - $topShift;
                int $postSpan = $length * $scale;
                int $bottomFixed = $topFixed + $postSpan;
                int $roundedTop = $topFixed + 65535;
                int $firstRow = $roundedTop >> 16;
                int $lastFixed = $bottomFixed - 1;
                int $lastRow = $lastFixed >> 16;
                if ($firstRow < $clipTop) {
                    $firstRow = $clipTop;
                }
                if ($lastRow > $clipBottom) {
                    $lastRow = $clipBottom;
                }
                int $endRow = $lastRow + 1;
                for ($y = $firstRow; $y < $endRow; $y++) {
                    int $fromCenter = $y - $centerY;
                    int $down = $fromCenter * $iscale;
                    int $v = $postMid + $down;
                    int $row = $v >> 16;
                    if ($row < 0 || $row >= $length) {
                        continue;
                    }
                    int $texelAt = $pixelsAt + $row;
                    int $colorAt = $colormap + $data[$texelAt];
                    int $screenRow = $y * $w;
                    int $screenAt = $screenRow + $x;
                    $screen[$screenAt] = $colormaps[$colorAt];
                }
                int $postBytes = $length + 4;
                int $next = $post + $postBytes;
                $post = $next;
            }
        }
    }

    // Visible rows per column for a sprite at ($thingX, $thingY) and $depth.
    public function clipColumns(int $thingX, int $thingY, int $x1, int $x2, int $depth): void {
        BspRenderer $world = $this->world;
        buffer<int> $columnTop = $this->columnTop;
        buffer<int> $columnBottom = $this->columnBottom;
        int $end = $x2 + 1;
        for ($x = $x1; $x < $end; $x++) {
            $columnTop[$x] = -2;
        }
        buffer<Seg> $segs = $world->segs;
        int $lastIndex = $world->drawSegCount - 1;
        for ($k = 0; $k < $world->drawSegCount; $k++) {
            int $ds = $lastIndex - $k;
            int $dsX1 = $world->drawSegX1[$ds];
            int $dsX2 = $world->drawSegX2[$ds];
            if ($dsX1 > $x2 || $dsX2 < $x1) {
                continue;
            }
            int $from = $dsX1;
            if ($from < $x1) {
                $from = $x1;
            }
            int $to = $dsX2;
            if ($to > $x2) {
                $to = $x2;
            }
            bool $behind = $world->drawSegNear[$ds] >= $depth;
            if (!$behind && $world->drawSegFar[$ds] > $depth) {
                // depth ranges overlap: the seg hides the sprite only if the sprite is behind its line
                int $segIndex = $world->drawSegIndex[$ds];
                int $edgeX = $segs[$segIndex]->x2 - $segs[$segIndex]->x1;
                int $edgeY = $segs[$segIndex]->y2 - $segs[$segIndex]->y1;
                int $toThingX = $thingX - $segs[$segIndex]->x1;
                int $toThingY = $thingY - $segs[$segIndex]->y1;
                int $crossA = $edgeX * $toThingY;
                int $crossB = $edgeY * $toThingX;
                $behind = $crossA < $crossB;
            }
            if ($behind) {
                $world->drawMaskedRange($ds, $from, $to);
                continue;
            }
            int $toEnd = $to + 1;
            bool $solid = $world->drawSegSolid[$ds] !== 0;
            int $opening = $world->drawSegOpening[$ds];
            for ($x = $from; $x < $toEnd; $x++) {
                if ($columnTop[$x] !== -2) {
                    continue;
                }
                if ($solid) {
                    $columnTop[$x] = 1;
                    $columnBottom[$x] = 0;
                    continue;
                }
                int $column = $x - $dsX1;
                int $at = $opening + $column;
                $columnTop[$x] = $world->openingTop[$at];
                $columnBottom[$x] = $world->openingBottom[$at];
            }
        }
        int $lastRow = $world->viewHeight - 1;
        for ($x = $x1; $x < $end; $x++) {
            if ($columnTop[$x] === -2) {
                $columnTop[$x] = 0;
                $columnBottom[$x] = $lastRow;
            }
        }
    }
}
