<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\Data\Linedef;
use Showcases\Doom\Data\Sector;
use Showcases\Doom\Data\Seg;
use Showcases\Doom\Data\Sidedef;
use Showcases\Doom\Data\SubSector;
use Showcases\Doom\Graphics\Flats;
use Showcases\Doom\Graphics\Palette;
use Showcases\Doom\Graphics\Textures;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Wad\Wad;

// The 3D view, drawn front to back one screen column at a time.
//
// Every column keeps the rows still open ($clipTop..$clipBottom). A seg draws, in
// that column, its front sector's ceiling above the wall and floor below it, then
// either a one-sided wall that closes the column or the upper and lower walls of a
// two-sided line, which narrow the opening for whatever lies behind. The BSP walk
// stops once every column is closed, so each pixel is written once.
//
// A two-sided line's middle texture (a grate, a fence) is see-through, so it cannot
// close anything: the seg saves, per column, the texture column and depth inside the
// opening it leaves, and drawMaskedRange() paints it later, back to front with the
// sprites, skipping the texels no patch covered.
//
// Fixed point: view-space coordinates are 16.16 world units; depth is the distance
// along the view direction. A wall texel row is texturemid + (y - centerY) * depth / focal;
// a flat pixel's distance is planeHeight * focal / (rows from the horizon), and its
// world position follows that distance along the column's ray.
final class BspRenderer {
    public buffer<u8> $screen;
    public int $width;
    public int $viewHeight;
    public int $centerX;
    public int $centerY;
    public int $centerYFixed;
    public int $focal;
    public int $focalFixed;
    public int $focalScale;
    public int $nearDepth;

    public buffer<int> $clipTop;
    public buffer<int> $clipBottom;
    public buffer<int> $solidDepth;
    public int $openColumns;
    public buffer<int> $yslope;
    public buffer<int> $xToAngle;
    public buffer<int> $columnOffset;
    public buffer<int> $rayX;
    public buffer<int> $rayY;

    // drawn segs that can hide sprites: screen span, depth range and, per column,
    // the open rows left after the seg (two-sided) or nothing at all (solid)
    public int $drawSegCount;
    public buffer<int> $drawSegX1;
    public buffer<int> $drawSegX2;
    public buffer<int> $drawSegNear;
    public buffer<int> $drawSegFar;
    public buffer<int> $drawSegIndex;
    public buffer<int> $drawSegSolid;
    public buffer<int> $drawSegOpening;
    public buffer<int> $openingTop;
    public buffer<int> $openingBottom;
    public int $openingUsed;
    // see-through middle textures: per drawn seg the texture (or -1), its 16.16 anchor
    // and its light row; per column of the seg's opening, the texture column still to
    // draw (or -1) and the depth there
    public buffer<int> $drawSegMasked;
    public buffer<int> $drawSegMaskedMid;
    public buffer<int> $drawSegMaskedLight;
    public buffer<int> $maskedColumn;
    public buffer<int> $maskedDepth;

    public int $viewX;
    public int $viewY;
    public int $viewZ;
    public int $viewAngle;
    public int $viewCos;
    public int $viewSin;
    public int $viewXFixed;
    public int $viewYFixed;

    public buffer<Seg> $segs;
    public buffer<Sector> $sectors;
    public buffer<Sidedef> $sidedefs;
    public buffer<Linedef> $linedefs;
    public buffer<SubSector> $subSectors;
    public int $segCount;

    public buffer<u8> $wadData;
    public buffer<u8> $texels;
    public buffer<u8> $opaque;
    public buffer<int> $textureWidth;
    public buffer<int> $textureHeight;
    public buffer<int> $textureOffset;
    public buffer<int> $textureTranslation;
    public buffer<int> $flatOffset;
    public buffer<int> $flatTranslation;
    public int $skyFlat;
    public int $skyTexture;
    public buffer<u8> $colormaps;
    public buffer<int> $wallLight;
    public buffer<int> $planeLight;
    public LightTables $lights;

    public function __construct(Screen $screen, int $viewHeight, Wad $wad, Palette $palette, Textures $textures, Flats $flats, LightTables $lights) {
        $this->screen = $screen->pixels;
        $this->width = $screen->width;
        $this->viewHeight = $viewHeight;
        $this->centerX = intdiv($screen->width, 2);
        $this->centerY = intdiv($viewHeight, 2);
        $this->centerYFixed = $this->centerY << 16;
        // a 90-degree field of view: the focal length is half the screen width
        $this->focal = $this->centerX;
        $this->focalFixed = $this->focal << 16;
        $this->focalScale = $this->focal << 32;
        $this->nearDepth = 4 << 16;

        $this->clipTop = buffer_new<int>($screen->width);
        $this->clipBottom = buffer_new<int>($screen->width);
        $this->solidDepth = buffer_new<int>($screen->width);
        $this->openColumns = 0;
        $this->rayX = buffer_new<int>($screen->width);
        $this->rayY = buffer_new<int>($screen->width);
        $this->xToAngle = buffer_new<int>($screen->width);
        $this->columnOffset = buffer_new<int>($screen->width);
        $this->yslope = buffer_new<int>($viewHeight);
        $this->drawSegCount = 0;
        $this->drawSegX1 = buffer_new<int>(1024);
        $this->drawSegX2 = buffer_new<int>(1024);
        $this->drawSegNear = buffer_new<int>(1024);
        $this->drawSegFar = buffer_new<int>(1024);
        $this->drawSegIndex = buffer_new<int>(1024);
        $this->drawSegSolid = buffer_new<int>(1024);
        $this->drawSegOpening = buffer_new<int>(1024);
        $this->openingTop = buffer_new<int>(131072);
        $this->openingBottom = buffer_new<int>(131072);
        $this->openingUsed = 0;
        $this->drawSegMasked = buffer_new<int>(1024);
        $this->drawSegMaskedMid = buffer_new<int>(1024);
        $this->drawSegMaskedLight = buffer_new<int>(1024);
        $this->maskedColumn = buffer_new<int>(131072);
        $this->maskedDepth = buffer_new<int>(131072);
        for ($x = 0; $x < $screen->width; $x++) {
            // pixel centers: offset (x + 0.5 - centerX) / focal
            float $offset = ($x + 0.5 - $this->centerX) / $this->focal;
            $this->columnOffset[$x] = (int)round($offset * 65536.0);
            $this->xToAngle[$x] = (int)round(-atan($offset) * 8192.0 / (2.0 * M_PI));
        }
        for ($y = 0; $y < $viewHeight; $y++) {
            float $rows = abs($y + 0.5 - $this->centerY);
            $this->yslope[$y] = (int)round($this->focal / $rows * 65536.0);
        }

        // the map's record buffers arrive with useMap(), before the first frame
        $this->segCount = 0;
        $this->wadData = $wad->data;
        $this->texels = $textures->texels;
        $this->opaque = $textures->opaque;
        $this->textureWidth = $textures->width;
        $this->textureHeight = $textures->height;
        $this->textureOffset = $textures->offset;
        $this->textureTranslation = $textures->translation;
        $this->flatOffset = $flats->offset;
        $this->flatTranslation = $flats->translation;
        $this->skyFlat = $flats->sky;
        $this->skyTexture = $textures->sky;
        $this->colormaps = $palette->colormaps;
        $this->wallLight = $lights->wall;
        $this->planeLight = $lights->plane;
        $this->lights = $lights;
        $this->viewX = 0;
        $this->viewY = 0;
        $this->viewZ = 0;
        $this->viewAngle = 0;
        $this->viewCos = 65536;
        $this->viewSin = 0;
        $this->viewXFixed = 0;
        $this->viewYFixed = 0;
    }

    public function useMap(MapData $map): void {
        $this->segs = $map->segs;
        $this->sectors = $map->sectors;
        $this->sidedefs = $map->sidedefs;
        $this->linedefs = $map->linedefs;
        $this->subSectors = $map->subSectors;
        $this->segCount = $map->segCount;
    }

    public function beginFrame(int $viewX, int $viewY, int $viewZ, int $viewAngle, int $cos, int $sin): void {
        $this->viewX = $viewX;
        $this->viewY = $viewY;
        $this->viewZ = $viewZ;
        $this->viewAngle = $viewAngle;
        $this->viewCos = $cos;
        $this->viewSin = $sin;
        $this->viewXFixed = $viewX << 16;
        $this->viewYFixed = $viewY << 16;

        buffer<int> $tops = $this->clipTop;
        buffer<int> $bottoms = $this->clipBottom;
        buffer<int> $depths = $this->solidDepth;
        buffer<int> $offsets = $this->columnOffset;
        buffer<int> $rayX = $this->rayX;
        buffer<int> $rayY = $this->rayY;
        int $lastRow = $this->viewHeight - 1;
        for ($x = 0; $x < $this->width; $x++) {
            $tops[$x] = 0;
            $bottoms[$x] = $lastRow;
            $depths[$x] = 9223372036854775807;
            // ray = forward + right * offset, with right = (sin, -cos)
            int $offset = $offsets[$x];
            int $sideX = $offset * $sin;
            int $sideY = $offset * $cos;
            int $rightX = $sideX >> 16;
            int $rightY = $sideY >> 16;
            int $directionX = $cos + $rightX;
            int $directionY = $sin - $rightY;
            $rayX[$x] = $directionX;
            $rayY[$x] = $directionY;
        }
        $this->openColumns = $this->width;
        $this->drawSegCount = 0;
        $this->openingUsed = 0;
    }

    public function renderSubSector(int $subSector): void {
        int $first = $this->subSectors[$subSector]->first_seg;
        int $count = $this->subSectors[$subSector]->seg_count;
        int $end = $first + $count;
        if ($end > $this->segCount) {
            $end = $this->segCount;
        }
        for ($segIndex = $first; $segIndex < $end; $segIndex++) {
            $this->renderSeg($segIndex);
        }
    }

    // Columns nothing covered (map holes) are painted black instead of keeping stale pixels.
    public function finishFrame(): void {
        buffer<int> $tops = $this->clipTop;
        buffer<int> $bottoms = $this->clipBottom;
        buffer<u8> $screen = $this->screen;
        int $w = $this->width;
        for ($x = 0; $x < $w; $x++) {
            int $top = $tops[$x];
            int $end = $bottoms[$x] + 1;
            for ($y = $top; $y < $end; $y++) {
                int $row = $y * $w;
                int $at = $row + $x;
                $screen[$at] = 0;
            }
        }
    }

    public function renderSeg(int $segIndex): void {
        buffer<Seg> $segs = $this->segs;
        int $frontSector = $segs[$segIndex]->front_sector;
        int $sidedef = $segs[$segIndex]->sidedef;
        if ($frontSector < 0 || $sidedef < 0) {
            return;
        }
        int $worldX1 = $segs[$segIndex]->x1;
        int $worldY1 = $segs[$segIndex]->y1;
        int $worldX2 = $segs[$segIndex]->x2;
        int $worldY2 = $segs[$segIndex]->y2;

        // only the front (right-hand) side of a seg is visible
        int $edgeX = $worldX2 - $worldX1;
        int $edgeY = $worldY2 - $worldY1;
        int $toViewX = $this->viewX - $worldX1;
        int $toViewY = $this->viewY - $worldY1;
        int $crossA = $edgeX * $toViewY;
        int $crossB = $edgeY * $toViewX;
        if ($crossA >= $crossB) {
            return;
        }

        int $relX1 = $worldX1 - $this->viewX;
        int $relY1 = $worldY1 - $this->viewY;
        int $relX2 = $worldX2 - $this->viewX;
        int $relY2 = $worldY2 - $this->viewY;
        int $depth1 = $this->depthOf($relX1, $relY1);
        int $depth2 = $this->depthOf($relX2, $relY2);
        int $side1 = $this->sideOf($relX1, $relY1);
        int $side2 = $this->sideOf($relX2, $relY2);
        int $near = $this->nearDepth;
        if ($depth1 < $near && $depth2 < $near) {
            return;
        }

        // screen span from the ends cut at the near plane
        int $clippedSide1 = $side1;
        int $clippedDepth1 = $depth1;
        int $clippedSide2 = $side2;
        int $clippedDepth2 = $depth2;
        if ($depth1 < $near) {
            $clippedSide1 = $this->sideAtDepth($side1, $depth1, $side2, $depth2, $near);
            $clippedDepth1 = $near;
        }
        if ($depth2 < $near) {
            $clippedSide2 = $this->sideAtDepth($side2, $depth2, $side1, $depth1, $near);
            $clippedDepth2 = $near;
        }
        int $screenX1 = $this->screenX($clippedSide1, $clippedDepth1);
        int $screenX2 = $this->screenX($clippedSide2, $clippedDepth2);
        if ($screenX1 >= $screenX2) {
            return;
        }
        int $roundedStart = $screenX1 + 65535;
        int $roundedEnd = $screenX2 + 65535;
        int $xStart = $roundedStart >> 16;
        int $xEnd = $roundedEnd >> 16;
        if ($xStart < 0) {
            $xStart = 0;
        }
        if ($xEnd > $this->width) {
            $xEnd = $this->width;
        }
        if ($xStart >= $xEnd) {
            return;
        }
        $this->linedefs[$segs[$segIndex]->linedef]->mapped = 1;

        // sectors and the sky hack (no upper wall between two sky ceilings)
        buffer<Sector> $sectors = $this->sectors;
        int $viewZ = $this->viewZ;
        int $frontFloor = $sectors[$frontSector]->floor_height;
        int $frontCeiling = $sectors[$frontSector]->ceiling_height;
        int $floorPic = $sectors[$frontSector]->floor_pic;
        int $ceilingPic = $sectors[$frontSector]->ceiling_pic;
        int $lightLevel = $sectors[$frontSector]->light_level;
        int $backSector = $segs[$segIndex]->back_sector;
        bool $twoSided = $backSector >= 0;
        int $backFloor = $frontFloor;
        int $backCeiling = $frontCeiling;
        int $worldTop = $frontCeiling;
        if ($twoSided) {
            $backFloor = $sectors[$backSector]->floor_height;
            $backCeiling = $sectors[$backSector]->ceiling_height;
            if ($ceilingPic === $this->skyFlat && $sectors[$backSector]->ceiling_pic === $this->skyFlat) {
                $worldTop = $backCeiling;
            }
        }
        bool $ceilingSky = $ceilingPic === $this->skyFlat;
        bool $markCeiling = ($frontCeiling > $viewZ || $ceilingSky) && $ceilingPic >= 0;
        bool $markFloor = $frontFloor < $viewZ && $floorPic >= 0;
        bool $hasUpper = $twoSided && $backCeiling < $worldTop;
        bool $hasLower = $twoSided && $backFloor > $frontFloor;

        // textures, with DOOM's pegging rules for where each texture's top row sits
        buffer<Sidedef> $sides = $this->sidedefs;
        int $linedef = $segs[$segIndex]->linedef;
        int $flags = $this->linedefs[$linedef]->flags;
        bool $upperUnpegged = ($flags & 8) !== 0;
        bool $lowerUnpegged = ($flags & 16) !== 0;
        int $rowOffset = $sides[$sidedef]->y_offset;
        int $textureU = $sides[$sidedef]->x_offset + $segs[$segIndex]->offset;
        int $segLength = $segs[$segIndex]->length;
        int $middleTexture = -1;
        int $upperTexture = -1;
        int $lowerTexture = -1;
        int $middleMid = 0;
        int $upperMid = 0;
        int $lowerMid = 0;
        if (!$twoSided) {
            $middleTexture = $this->drawnTexture($sides[$sidedef]->middle);
            int $middleTop = $worldTop;
            if ($lowerUnpegged && $middleTexture >= 0) {
                int $raised = $frontFloor + $this->textureHeight[$middleTexture];
                $middleTop = $raised;
            }
            $middleMid = $this->textureMid($middleTop, $rowOffset);
        }
        // a two-sided middle texture hangs once, not tiled, from the lower ceiling or,
        // lower-unpegged, stands on the higher floor
        int $maskedTexture = -1;
        int $maskedMid = 0;
        int $maskedWidth = 1;
        if ($twoSided) {
            $maskedTexture = $this->drawnTexture($sides[$sidedef]->middle);
        }
        if ($maskedTexture >= 0) {
            $maskedWidth = $this->textureWidth[$maskedTexture];
            int $maskedTop = $frontCeiling;
            if ($backCeiling < $maskedTop) {
                $maskedTop = $backCeiling;
            }
            if ($lowerUnpegged) {
                int $maskedFloor = $frontFloor;
                if ($backFloor > $maskedFloor) {
                    $maskedFloor = $backFloor;
                }
                int $standing = $maskedFloor + $this->textureHeight[$maskedTexture];
                $maskedTop = $standing;
            }
            $maskedMid = $this->textureMid($maskedTop, $rowOffset);
        }
        if ($hasUpper) {
            $upperTexture = $this->drawnTexture($sides[$sidedef]->upper);
            int $upperTop = $worldTop;
            if (!$upperUnpegged && $upperTexture >= 0) {
                int $hung = $backCeiling + $this->textureHeight[$upperTexture];
                $upperTop = $hung;
            }
            $upperMid = $this->textureMid($upperTop, $rowOffset);
        }
        if ($hasLower) {
            $lowerTexture = $this->drawnTexture($sides[$sidedef]->lower);
            int $lowerTop = $backFloor;
            if ($lowerUnpegged) {
                $lowerTop = $worldTop;
            }
            $lowerMid = $this->textureMid($lowerTop, $rowOffset);
        }

        // light: sector band, fake contrast on axis-aligned walls, then distance
        int $contrast = 0;
        if ($worldY1 === $worldY2) {
            $contrast = -1;
        } else if ($worldX1 === $worldX2) {
            $contrast = 1;
        }
        int $wallBand = $this->lights->band($lightLevel, $contrast);
        int $planeBand = $this->lights->band($lightLevel, 0);
        int $wallRow = $wallBand * 48;
        int $planeRow = $planeBand * 128;
        int $floorFlat = -1;
        int $ceilingFlat = -1;
        if ($markFloor) {
            $floorFlat = $this->flatOffset[$this->flatTranslation[$floorPic]];
        }
        if ($markCeiling && !$ceilingSky) {
            $ceilingFlat = $this->flatOffset[$this->flatTranslation[$ceilingPic]];
        }
        int $floorHeight = $viewZ - $frontFloor;
        int $ceilingHeight = $frontCeiling - $viewZ;
        int $topHeight = $worldTop - $viewZ;
        int $bottomHeight = $frontFloor - $viewZ;
        int $backTopHeight = $backCeiling - $viewZ;
        int $backBottomHeight = $backFloor - $viewZ;

        // per-column intersection of the column's ray with the (unclipped) seg line
        int $deltaSide = $side2 - $side1;
        int $deltaDepth = $depth2 - $depth1;
        int $focal = $this->focal;
        int $focalSide = $focal * $side1;
        int $focalDelta = $focal * $deltaSide;
        int $centerX = $this->centerX;
        int $centerYFixed = $this->centerYFixed;
        int $focalScale = $this->focalScale;
        buffer<int> $tops = $this->clipTop;
        buffer<int> $bottoms = $this->clipBottom;
        buffer<int> $wallLight = $this->wallLight;
        // the masked columns sit where recordDrawSeg() will put this seg's opening
        bool $masked = $maskedTexture >= 0;
        int $maskedStart = $this->openingUsed;
        int $span = $xEnd - $xStart;
        int $maskedEnd = $maskedStart + $span;
        if ($maskedEnd > 131072) {
            $masked = false;
        }
        buffer<int> $maskedColumn = $this->maskedColumn;
        buffer<int> $maskedDepth = $this->maskedDepth;
        if ($masked) {
            for ($m = $maskedStart; $m < $maskedEnd; $m++) {
                $maskedColumn[$m] = -1;
            }
        }
        for ($x = $xStart; $x < $xEnd; $x++) {
            int $clipTop = $tops[$x];
            int $clipBottom = $bottoms[$x];
            if ($clipTop > $clipBottom) {
                continue;
            }

            int $column = $x - $centerX;
            int $rayDepth = $column * $depth1;
            int $rayDelta = $column * $deltaDepth;
            int $numerator = $rayDepth - $focalSide;
            int $denominator = $focalDelta - $rayDelta;
            if ($denominator === 0) {
                continue;
            }
            int $scaledNumerator = $numerator << 16;
            int $t = intdiv($scaledNumerator, $denominator);
            if ($t < 0) {
                $t = 0;
            } else if ($t > 65536) {
                $t = 65536;
            }
            int $depthTravel = $t * $deltaDepth;
            int $depthStep = $depthTravel >> 16;
            int $depth = $depth1 + $depthStep;
            if ($depth < $near) {
                $depth = $near;
            }
            int $lengthTravel = $t * $segLength;
            int $along = $lengthTravel >> 16;
            int $u = $textureU + $along;

            // scale = focal / depth (16.16); one world unit of height spans scale pixels
            int $scale = intdiv($focalScale, $depth);
            int $iscale = intdiv($depth, $focal);
            int $scaleIndex = $scale >> 12;
            if ($scaleIndex > 47) {
                $scaleIndex = 47;
            }
            int $wallSlot = $wallRow + $scaleIndex;
            int $wallColormap = $wallLight[$wallSlot];

            int $topRow = $this->rowOf($topHeight, $scale);
            int $bottomRow = $this->rowOf($bottomHeight, $scale);

            // ceiling above the wall and floor below it, inside the open rows
            int $ceilingEnd = $topRow - 1;
            if ($ceilingEnd > $clipBottom) {
                $ceilingEnd = $clipBottom;
            }
            if ($markCeiling && $ceilingEnd >= $clipTop) {
                if ($ceilingSky) {
                    $this->drawSky($x, $clipTop, $ceilingEnd);
                } else {
                    $this->drawPlane($x, $clipTop, $ceilingEnd, $ceilingFlat, $ceilingHeight, $planeRow);
                }
            }
            int $floorStart = $bottomRow;
            if ($floorStart < $clipTop) {
                $floorStart = $clipTop;
            }
            if ($markFloor && $floorStart <= $clipBottom) {
                $this->drawPlane($x, $floorStart, $clipBottom, $floorFlat, $floorHeight, $planeRow);
            }

            int $wallTop = $topRow;
            if ($wallTop < $clipTop) {
                $wallTop = $clipTop;
            }
            int $wallBottom = $bottomRow - 1;
            if ($wallBottom > $clipBottom) {
                $wallBottom = $clipBottom;
            }
            if (!$twoSided) {
                $this->drawWall($x, $wallTop, $wallBottom, $middleTexture, $u, $middleMid, $iscale, $wallColormap);
                $this->closeColumn($x, $depth);
                continue;
            }

            int $openTop = $wallTop;
            int $openBottom = $wallBottom;
            if ($hasUpper) {
                int $backTopRow = $this->rowOf($backTopHeight, $scale);
                int $upperBottom = $backTopRow - 1;
                if ($upperBottom > $clipBottom) {
                    $upperBottom = $clipBottom;
                }
                $this->drawWall($x, $wallTop, $upperBottom, $upperTexture, $u, $upperMid, $iscale, $wallColormap);
                if ($backTopRow > $openTop) {
                    $openTop = $backTopRow;
                }
            }
            if ($hasLower) {
                int $backBottomRow = $this->rowOf($backBottomHeight, $scale);
                int $lowerTop = $backBottomRow;
                if ($lowerTop < $clipTop) {
                    $lowerTop = $clipTop;
                }
                $this->drawWall($x, $lowerTop, $wallBottom, $lowerTexture, $u, $lowerMid, $iscale, $wallColormap);
                int $aboveLower = $backBottomRow - 1;
                if ($aboveLower < $openBottom) {
                    $openBottom = $aboveLower;
                }
            }
            if ($openTop > $openBottom) {
                $this->closeColumn($x, $depth);
            } else {
                $tops[$x] = $openTop;
                $bottoms[$x] = $openBottom;
                if ($masked) {
                    int $fromStart = $x - $xStart;
                    int $maskedAt = $maskedStart + $fromStart;
                    int $uWrapped = $u % $maskedWidth;
                    int $uShifted = $uWrapped + $maskedWidth;
                    $maskedColumn[$maskedAt] = $uShifted % $maskedWidth;
                    $maskedDepth[$maskedAt] = $depth;
                }
            }
        }

        if (!$twoSided || $hasUpper || $hasLower || $masked) {
            int $maskedSlot = -1;
            if ($masked) {
                $maskedSlot = $maskedTexture;
            }
            $this->recordDrawSeg($segIndex, $xStart, $xEnd, $clippedDepth1, $clippedDepth2, !$twoSided, $maskedSlot, $maskedMid, $wallRow);
        }
    }

    public function recordDrawSeg(int $segIndex, int $xStart, int $xEnd, int $depth1, int $depth2, bool $solid, int $maskedTexture, int $maskedMid, int $maskedLight): void {
        int $slot = $this->drawSegCount;
        if ($slot >= 1024) {
            return;
        }
        int $span = $xEnd - $xStart;
        int $openingStart = $this->openingUsed;
        int $openingEnd = $openingStart + $span;
        if (!$solid && $openingEnd > 131072) {
            return;
        }
        int $near = $depth1;
        int $far = $depth2;
        if ($depth2 < $depth1) {
            $near = $depth2;
            $far = $depth1;
        }
        int $lastX = $xEnd - 1;
        $this->drawSegX1[$slot] = $xStart;
        $this->drawSegX2[$slot] = $lastX;
        $this->drawSegNear[$slot] = $near;
        $this->drawSegFar[$slot] = $far;
        $this->drawSegIndex[$slot] = $segIndex;
        $this->drawSegOpening[$slot] = $openingStart;
        $this->drawSegMasked[$slot] = $maskedTexture;
        $this->drawSegMaskedMid[$slot] = $maskedMid;
        $this->drawSegMaskedLight[$slot] = $maskedLight;
        $this->drawSegSolid[$slot] = 0;
        if ($solid) {
            $this->drawSegSolid[$slot] = 1;
        } else {
            buffer<int> $tops = $this->clipTop;
            buffer<int> $bottoms = $this->clipBottom;
            buffer<int> $openingTop = $this->openingTop;
            buffer<int> $openingBottom = $this->openingBottom;
            for ($x = $xStart; $x < $xEnd; $x++) {
                int $column = $x - $xStart;
                int $at = $openingStart + $column;
                $openingTop[$at] = $tops[$x];
                $openingBottom[$at] = $bottoms[$x];
            }
            $this->openingUsed = $openingEnd;
        }
        $this->drawSegCount++;
    }

    // First screen row at or below a height that sits $height units above the eye.
    public function rowOf(int $height, int $scale): int {
        int $rise = $height * $scale;
        int $fixed = $this->centerYFixed - $rise;
        int $rounded = $fixed + 65535;
        int $row = $rounded >> 16;
        return $row;
    }

    public function depthOf(int $relX, int $relY): int {
        int $alongX = $relX * $this->viewCos;
        int $alongY = $relY * $this->viewSin;
        int $depth = $alongX + $alongY;
        return $depth;
    }

    public function sideOf(int $relX, int $relY): int {
        int $acrossX = $relX * $this->viewSin;
        int $acrossY = $relY * $this->viewCos;
        int $side = $acrossX - $acrossY;
        return $side;
    }

    // Side coordinate where the segment from (side, depth) to (otherSide, otherDepth) reaches $depthTarget.
    public function sideAtDepth(int $side, int $depth, int $otherSide, int $otherDepth, int $depthTarget): int {
        int $span = $otherDepth - $depth;
        if ($span === 0) {
            return $side;
        }
        int $reach = $depthTarget - $depth;
        int $sideSpan = $otherSide - $side;
        int $shifted = $sideSpan * $reach;
        int $step = intdiv($shifted, $span);
        int $result = $side + $step;
        return $result;
    }

    // Screen x in 16.16 of a view-space point.
    public function screenX(int $side, int $depth): int {
        int $scaled = $side * $this->focal;
        int $shifted = $scaled << 16;
        int $offset = intdiv($shifted, $depth);
        int $centerFixed = $this->centerX << 16;
        int $x = $centerFixed + $offset;
        return $x;
    }

    // Texture row 0 relative to the eye, plus the sidedef's row offset, in 16.16.
    public function textureMid(int $top, int $rowOffset): int {
        int $relative = $top - $this->viewZ;
        int $shifted = $relative + $rowOffset;
        int $mid = $shifted << 16;
        return $mid;
    }

    public function drawnTexture(int $texture): int {
        if ($texture < 0) {
            return -1;
        }
        return $this->textureTranslation[$texture];
    }

    public function closeColumn(int $x, int $depth): void {
        $this->clipTop[$x] = $this->viewHeight;
        $this->clipBottom[$x] = -1;
        $this->solidDepth[$x] = $depth;
        $this->openColumns--;
    }

    public function drawWall(int $x, int $top, int $bottom, int $texture, int $u, int $mid, int $iscale, int $colormap): void {
        if ($top > $bottom || $texture < 0) {
            return;
        }
        buffer<u8> $screen = $this->screen;
        buffer<u8> $texels = $this->texels;
        buffer<u8> $colormaps = $this->colormaps;
        int $width = $this->textureWidth[$texture];
        int $height = $this->textureHeight[$texture];
        int $uWrapped = $u % $width;
        int $uShifted = $uWrapped + $width;
        int $textureColumn = $uShifted % $width;
        int $columnStart = $textureColumn * $height;
        int $columnBase = $this->textureOffset[$texture] + $columnStart;
        int $centerY = $this->centerY;
        int $w = $this->width;
        int $end = $bottom + 1;
        int $heightMask = $height - 1;
        int $powerTest = $height & $heightMask;
        if ($powerTest === 0) {
            for ($y = $top; $y < $end; $y++) {
                int $fromCenter = $y - $centerY;
                int $travel = $fromCenter * $iscale;
                int $v = $mid + $travel;
                int $texelRow = $v >> 16;
                int $row = $texelRow & $heightMask;
                int $texelAt = $columnBase + $row;
                int $colorAt = $colormap + $texels[$texelAt];
                int $screenRow = $y * $w;
                int $screenAt = $screenRow + $x;
                $screen[$screenAt] = $colormaps[$colorAt];
            }
            return;
        }
        for ($y = $top; $y < $end; $y++) {
            int $fromCenter = $y - $centerY;
            int $travel = $fromCenter * $iscale;
            int $v = $mid + $travel;
            int $texelRow = $v >> 16;
            int $wrapped = $texelRow % $height;
            int $shifted = $wrapped + $height;
            int $row = $shifted % $height;
            int $texelAt = $columnBase + $row;
            int $colorAt = $colormap + $texels[$texelAt];
            int $screenRow = $y * $w;
            int $screenAt = $screenRow + $x;
            $screen[$screenAt] = $colormaps[$colorAt];
        }
    }

    // The still undrawn columns of drawn seg $ds's middle texture between screen
    // columns $from and $to, each clipped to the opening the seg left there. A drawn
    // column is marked done, so the final pass does not paint it over a sprite.
    public function drawMaskedRange(int $ds, int $from, int $to): void {
        int $texture = $this->drawSegMasked[$ds];
        if ($texture < 0) {
            return;
        }
        buffer<int> $maskedColumn = $this->maskedColumn;
        buffer<int> $maskedDepth = $this->maskedDepth;
        buffer<int> $openingTop = $this->openingTop;
        buffer<int> $openingBottom = $this->openingBottom;
        buffer<int> $wallLight = $this->wallLight;
        buffer<u8> $screen = $this->screen;
        buffer<u8> $texels = $this->texels;
        buffer<u8> $opaque = $this->opaque;
        buffer<u8> $colormaps = $this->colormaps;
        int $x1 = $this->drawSegX1[$ds];
        int $opening = $this->drawSegOpening[$ds];
        int $mid = $this->drawSegMaskedMid[$ds];
        int $lightRow = $this->drawSegMaskedLight[$ds];
        int $height = $this->textureHeight[$texture];
        int $textureBase = $this->textureOffset[$texture];
        int $midUnits = $mid >> 16;
        int $bottomUnits = $midUnits - $height;
        int $centerY = $this->centerY;
        int $w = $this->width;
        int $focal = $this->focal;
        int $focalScale = $this->focalScale;
        int $end = $to + 1;
        for ($x = $from; $x < $end; $x++) {
            int $column = $x - $x1;
            int $at = $opening + $column;
            int $textureColumn = $maskedColumn[$at];
            if ($textureColumn < 0) {
                continue;
            }
            $maskedColumn[$at] = -1;
            int $depth = $maskedDepth[$at];
            int $scale = intdiv($focalScale, $depth);
            int $iscale = intdiv($depth, $focal);
            int $scaleIndex = $scale >> 12;
            if ($scaleIndex > 47) {
                $scaleIndex = 47;
            }
            int $lightSlot = $lightRow + $scaleIndex;
            int $colormap = $wallLight[$lightSlot];
            // the texture's rows 0 .. height - 1 fall between these screen rows
            int $top = $this->rowOf($midUnits, $scale);
            int $below = $this->rowOf($bottomUnits, $scale);
            if ($top < $openingTop[$at]) {
                $top = $openingTop[$at];
            }
            int $stop = $openingBottom[$at] + 1;
            if ($below < $stop) {
                $stop = $below;
            }
            int $columnStart = $textureColumn * $height;
            int $columnBase = $textureBase + $columnStart;
            for ($y = $top; $y < $stop; $y++) {
                int $fromCenter = $y - $centerY;
                int $travel = $fromCenter * $iscale;
                int $v = $mid + $travel;
                int $row = $v >> 16;
                if ($row < 0 || $row >= $height) {
                    continue;
                }
                int $texelAt = $columnBase + $row;
                if ($opaque[$texelAt] === 0) {
                    continue;
                }
                int $colorAt = $colormap + $texels[$texelAt];
                int $screenRow = $y * $w;
                int $screenAt = $screenRow + $x;
                $screen[$screenAt] = $colormaps[$colorAt];
            }
        }
    }

    // Whatever middle-texture columns no sprite needed drawn first, far to near.
    public function drawMaskedSegs(): void {
        int $lastIndex = $this->drawSegCount - 1;
        for ($k = 0; $k < $this->drawSegCount; $k++) {
            int $ds = $lastIndex - $k;
            if ($this->drawSegMasked[$ds] >= 0) {
                $this->drawMaskedRange($ds, $this->drawSegX1[$ds], $this->drawSegX2[$ds]);
            }
        }
    }

    // A floor or ceiling column: each pixel's distance comes from the plane height and
    // its row, its world position from the column's ray, its light from the distance.
    public function drawPlane(int $x, int $top, int $bottom, int $flat, int $height, int $planeRow): void {
        if ($flat < 0) {
            return;
        }
        buffer<u8> $screen = $this->screen;
        buffer<u8> $wadData = $this->wadData;
        buffer<u8> $colormaps = $this->colormaps;
        buffer<int> $yslope = $this->yslope;
        buffer<int> $planeLight = $this->planeLight;
        int $rayX = $this->rayX[$x];
        int $rayY = $this->rayY[$x];
        int $viewXFixed = $this->viewXFixed;
        int $viewYFixed = $this->viewYFixed;
        int $w = $this->width;
        int $end = $bottom + 1;
        for ($y = $top; $y < $end; $y++) {
            int $distance = $height * $yslope[$y];
            int $travelX = $distance * $rayX;
            int $travelY = $distance * $rayY;
            int $offsetX = $travelX >> 16;
            int $offsetY = $travelY >> 16;
            int $worldX = $viewXFixed + $offsetX;
            int $worldY = $viewYFixed + $offsetY;
            int $cellX = $worldX >> 16;
            int $flippedY = -$worldY;
            int $cellY = $flippedY >> 16;
            int $u = $cellX & 63;
            int $v = $cellY & 63;
            int $rowStart = $v << 6;
            int $texelIndex = $rowStart | $u;
            int $texelAt = $flat + $texelIndex;
            int $zone = $distance >> 20;
            if ($zone > 127) {
                $zone = 127;
            }
            int $lightSlot = $planeRow + $zone;
            int $colorAt = $planeLight[$lightSlot] + $wadData[$texelAt];
            int $screenRow = $y * $w;
            int $screenAt = $screenRow + $x;
            $screen[$screenAt] = $colormaps[$colorAt];
        }
    }

    // Sky: the column comes from the view angle (four repeats per turn), rows are fixed, full bright.
    public function drawSky(int $x, int $top, int $bottom): void {
        int $texture = $this->drawnTexture($this->skyTexture);
        if ($texture < 0) {
            return;
        }
        buffer<u8> $screen = $this->screen;
        buffer<u8> $texels = $this->texels;
        int $angle = $this->viewAngle + $this->xToAngle[$x];
        int $wrappedAngle = $angle & 8191;
        int $skyColumn = $wrappedAngle >> 3;
        int $width = $this->textureWidth[$texture];
        int $height = $this->textureHeight[$texture];
        int $textureColumn = $skyColumn % $width;
        int $columnStart = $textureColumn * $height;
        int $columnBase = $this->textureOffset[$texture] + $columnStart;
        int $rowShift = 100 - $this->centerY;
        int $w = $this->width;
        int $end = $bottom + 1;
        for ($y = $top; $y < $end; $y++) {
            int $skyRow = $y + $rowShift;
            int $row = $skyRow % $height;
            int $texelAt = $columnBase + $row;
            int $screenRow = $y * $w;
            int $screenAt = $screenRow + $x;
            $screen[$screenAt] = $texels[$texelAt];
        }
    }
}
