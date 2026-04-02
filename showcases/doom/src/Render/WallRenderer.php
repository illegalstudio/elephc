<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\App\Config;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Player\Camera;
use Showcases\Doom\SDL\SDL;
use Showcases\Doom\Support\Direction;

class WallRenderer {
    public $projection;
    public $direction;

    public function __construct() {
        $this->projection = new Projection();
        $this->direction = new Direction();
    }

    public function render(
        SDL $sdl,
        Config $config,
        MapData $map,
        Camera $camera,
        $subSectorOrder,
        int $cameraSubSector
    ): void {
        if (!$map->isValid() || $map->segCount <= 0 || $map->sidedefCount <= 0 || $map->sectorCount <= 0) {
            return;
        }

        int $viewportX = 0;
        int $viewportY = 0;
        int $viewportWidth = $config->windowWidth;
        int $viewportHeight = $config->windowHeight;
        int $centerX = intdiv($viewportWidth, 2);
        int $horizonY = intdiv($viewportHeight, 2);
        int $focal = intdiv($viewportWidth * 3, 4);
        int $nearPlane = 12;
        int $cameraEyeZ = $this->projection->cameraEyeZ($map, $cameraSubSector);
        int $maxY = $viewportY + $viewportHeight - 1;
        $clipData = [];

        int $clipIndex = 0;
        while ($clipIndex < $viewportWidth) {
            $clipData[] = 2147483647;
            $clipData[] = $viewportY;
            $clipData[] = $maxY;
            $clipIndex += 1;
        }

        $this->renderFlatBackground(
            $sdl,
            $viewportX,
            $viewportY,
            $viewportWidth,
            $viewportHeight,
            $horizonY
        );

        int $subSectorCount = count($subSectorOrder);
        int $orderIndex = 0;
        while ($orderIndex < $subSectorCount) {
            int $subSectorIndex = $subSectorOrder[$orderIndex];
            if ($subSectorIndex >= 0 && $subSectorIndex < $map->subSectorCount) {
                int $firstSeg = $map->subSectors[$subSectorIndex]->first_seg_index;
                int $segCount = $map->subSectors[$subSectorIndex]->seg_count;
                int $segOffset = 0;
                while ($segOffset < $segCount) {
                    int $segIndex = $firstSeg + $segOffset;
                    if ($segIndex >= 0 && $segIndex < $map->segCount) {
                        $this->renderSeg(
                            $sdl,
                            $map,
                            $camera,
                            $segIndex,
                            $viewportX,
                            $viewportY,
                            $viewportWidth,
                            $centerX,
                            $horizonY,
                            $focal,
                            $nearPlane,
                            $cameraEyeZ,
                            $viewportHeight,
                            $clipData
                        );
                    }
                    $segOffset += 1;
                }
            }
            $orderIndex += 1;
        }
    }

    public function renderSeg(
        SDL $sdl,
        MapData $map,
        Camera $camera,
        int $segIndex,
        int $viewportX,
        int $viewportY,
        int $viewportWidth,
        int $centerX,
        int $horizonY,
        int $focal,
        int $nearPlane,
        int $cameraEyeZ,
        int $viewportHeight,
        &$clipData
    ): void {
        int $startIndex = $map->segs[$segIndex]->start_vertex;
        int $endIndex = $map->segs[$segIndex]->end_vertex;
        if (
            $startIndex < 0
            || $endIndex < 0
            || $startIndex >= $map->vertexCount
            || $endIndex >= $map->vertexCount
        ) {
            return;
        }

        if (!$this->shouldRenderSeg($map, $camera, $segIndex, $startIndex, $endIndex)) {
            return;
        }

        int $worldX1 = $map->vertexes[$startIndex]->x;
        int $worldY1 = $map->vertexes[$startIndex]->y;
        int $worldX2 = $map->vertexes[$endIndex]->x;
        int $worldY2 = $map->vertexes[$endIndex]->y;

        int $relX1 = $worldX1 - $camera->x;
        int $relY1 = $worldY1 - $camera->y;
        int $relX2 = $worldX2 - $camera->x;
        int $relY2 = $worldY2 - $camera->y;

        int $forwardX = $this->direction->unitX($camera->angle);
        int $forwardY = $this->direction->unitY($camera->angle);
        int $rightX = -$forwardY;
        int $rightY = $forwardX;

        int $depth1 = intdiv(($relX1 * $forwardX) + ($relY1 * $forwardY), 1024);
        int $depth2 = intdiv(($relX2 * $forwardX) + ($relY2 * $forwardY), 1024);
        int $side1 = intdiv(($relX1 * $rightX) + ($relY1 * $rightY), 1024);
        int $side2 = intdiv(($relX2 * $rightX) + ($relY2 * $rightY), 1024);

        if ($depth1 <= $nearPlane && $depth2 <= $nearPlane) {
            return;
        }

        if ($depth1 <= $nearPlane) {
            int $den = $depth2 - $depth1;
            if ($den <= 0) {
                return;
            }
            int $num = $nearPlane - $depth1;
            if ($num > $den) {
                $num = $den;
            }
            $side1 = $side1 + intdiv(($side2 - $side1) * $num, $den);
            $depth1 = $nearPlane;
        }
        if ($depth2 <= $nearPlane) {
            int $den = $depth1 - $depth2;
            if ($den <= 0) {
                return;
            }
            int $num = $nearPlane - $depth2;
            if ($num > $den) {
                $num = $den;
            }
            $side2 = $side2 + intdiv(($side1 - $side2) * $num, $den);
            $depth2 = $nearPlane;
        }

        int $screenX1 = $viewportX + $centerX + intdiv($side1 * $focal, $depth1);
        int $screenX2 = $viewportX + $centerX + intdiv($side2 * $focal, $depth2);

        if ($screenX1 === $screenX2) {
            return;
        }

        int $leftX = $screenX1;
        int $rightScreenX = $screenX2;
        int $leftSide = $side1;
        int $rightSide = $side2;
        int $leftDepth = $depth1;
        int $rightDepth = $depth2;
        if ($leftX > $rightScreenX) {
            $leftX = $screenX2;
            $rightScreenX = $screenX1;
            $leftSide = $side2;
            $rightSide = $side1;
            $leftDepth = $depth2;
            $rightDepth = $depth1;
        }

        if ($rightScreenX < $viewportX || $leftX >= $viewportX + $viewportWidth) {
            return;
        }

        int $frontSectorIndex = $this->frontSectorIndexForSeg($map, $segIndex);
        if ($frontSectorIndex < 0 || $frontSectorIndex >= $map->sectorCount) {
            return;
        }

        int $frontFloor = $map->sectors[$frontSectorIndex]->floor_height;
        int $frontCeiling = $map->sectors[$frontSectorIndex]->ceiling_height;
        int $backSectorIndex = $this->backSectorIndexForSeg($map, $segIndex);
        bool $oneSided = $backSectorIndex < 0 || $backSectorIndex >= $map->sectorCount;
        int $backFloor = $frontFloor;
        int $backCeiling = $frontCeiling;
        if (!$oneSided) {
            $backFloor = $map->sectors[$backSectorIndex]->floor_height;
            $backCeiling = $map->sectors[$backSectorIndex]->ceiling_height;
        }

        int $light = $this->wallLightForSeg($map, $segIndex);
        int $distance = intdiv($leftDepth + $rightDepth, 2);
        int $segDx = $worldX2 - $worldX1;
        int $segDy = $worldY2 - $worldY1;
        bool $mostlyVertical = $this->absoluteValue($segDy) > $this->absoluteValue($segDx);

        // exponential distance fog: fade = dist² / (dist² + k²), k=800
        int $distSq = $distance * $distance;
        int $fogDenom = $distSq + 640000;
        int $fogFactor = 255;
        if ($fogDenom > 0) {
            $fogFactor = 255 - intdiv($distSq * 220, $fogDenom);
        }
        if ($fogFactor < 35) {
            $fogFactor = 35;
        }

        // sector light drives the base brightness strongly
        int $litR = 20 + intdiv($light * 140, 255);
        int $litG = 22 + intdiv($light * 150, 255);
        int $litB = 28 + intdiv($light * 130, 255);

        // wall orientation tint
        if ($mostlyVertical) {
            $litG += 6;
            $litB += 10;
        } else {
            $litR += 10;
            $litG += 4;
        }
        if ($map->segs[$segIndex]->direction != 0) {
            $litR += 4;
            $litB += 6;
        }

        // apply fog
        $litR = intdiv($litR * $fogFactor, 255);
        $litG = intdiv($litG * $fogFactor, 255);
        $litB = intdiv($litB * $fogFactor, 255);

        // mid wall color (one-sided walls: main surface)
        int $midRed = $litR;
        int $midGreen = $litG;
        int $midBlue = $litB;
        if ($oneSided) {
            $midRed += 6;
            $midGreen += 4;
        }

        // upper step: cooler, bluish tint (exposed to sky)
        int $upperRed = intdiv($litR * 3, 4);
        int $upperGreen = intdiv($litG * 4, 5);
        int $upperBlue = $litB + intdiv((255 - $litB), 4);

        // lower step: warmer, brownish tint (near ground)
        int $lowerRed = $litR + intdiv((255 - $litR), 5);
        int $lowerGreen = intdiv($litG * 4, 5);
        int $lowerBlue = intdiv($litB * 3, 5);

        // clamp all colors
        $midRed = $this->clampColor($midRed);
        $midGreen = $this->clampColor($midGreen);
        $midBlue = $this->clampColor($midBlue);
        $upperRed = $this->clampColor($upperRed);
        $upperGreen = $this->clampColor($upperGreen);
        $upperBlue = $this->clampColor($upperBlue);
        $lowerRed = $this->clampColor($lowerRed);
        $lowerGreen = $this->clampColor($lowerGreen);
        $lowerBlue = $this->clampColor($lowerBlue);

        if ($leftX < $viewportX) {
            $leftX = $viewportX;
        }
        if ($rightScreenX >= $viewportX + $viewportWidth) {
            $rightScreenX = ($viewportX + $viewportWidth) - 1;
        }

        int $x = $leftX;
        int $screenCenter = $viewportX + $centerX;
        while ($x <= $rightScreenX) {
            int $depth = $this->depthForColumn(
                $leftSide,
                $leftDepth,
                $rightSide,
                $rightDepth,
                $x,
                $screenCenter,
                $focal
            );
            if ($depth < $nearPlane) {
                $depth = $nearPlane;
            }

            int $clipColumn = $x - $viewportX;
            if ($clipColumn < 0 || $clipColumn >= $viewportWidth) {
                $x += 1;
                continue;
            }
            int $base = $clipColumn * 3;
            if ($depth >= $clipData[$base]) {
                $x += 1;
                continue;
            }

            int $idxCeil = $base + 1;
            int $idxFloor = $base + 2;
            int $colCeil = $clipData[$idxCeil];
            int $colFloor = $clipData[$idxFloor];
            if ($colCeil >= $colFloor) {
                $x += 1;
                continue;
            }

            if ($oneSided) {
                int $top = $this->projection->projectScreenY(
                    $frontCeiling,
                    $cameraEyeZ,
                    $depth,
                    $viewportY,
                    $horizonY,
                    $focal
                );
                int $bottom = $this->projection->projectScreenY(
                    $frontFloor,
                    $cameraEyeZ,
                    $depth,
                    $viewportY,
                    $horizonY,
                    $focal
                );
                $this->drawClippedSpan(
                    $sdl, $x, $top, $bottom,
                    $colCeil, $colFloor,
                    $midRed, $midGreen, $midBlue
                );
                $clipData[$base] = $depth;
            } else {
                if ($frontCeiling !== $backCeiling) {
                    int $top = $this->projection->projectScreenY(
                        $this->higherOf($frontCeiling, $backCeiling),
                        $cameraEyeZ,
                        $depth,
                        $viewportY,
                        $horizonY,
                        $focal
                    );
                    int $bottom = $this->projection->projectScreenY(
                        $this->lowerOf($frontCeiling, $backCeiling),
                        $cameraEyeZ,
                        $depth,
                        $viewportY,
                        $horizonY,
                        $focal
                    );
                    $this->drawClippedSpan(
                        $sdl, $x, $top, $bottom,
                        $colCeil, $colFloor,
                        $upperRed, $upperGreen, $upperBlue
                    );
                }
                if ($frontFloor !== $backFloor) {
                    int $top = $this->projection->projectScreenY(
                        $this->higherOf($frontFloor, $backFloor),
                        $cameraEyeZ,
                        $depth,
                        $viewportY,
                        $horizonY,
                        $focal
                    );
                    int $bottom = $this->projection->projectScreenY(
                        $this->lowerOf($frontFloor, $backFloor),
                        $cameraEyeZ,
                        $depth,
                        $viewportY,
                        $horizonY,
                        $focal
                    );
                    $this->drawClippedSpan(
                        $sdl, $x, $top, $bottom,
                        $colCeil, $colFloor,
                        $lowerRed, $lowerGreen, $lowerBlue
                    );
                }
                if ($backFloor >= $backCeiling) {
                    $clipData[$base] = $depth;
                }
            }
            $x += 1;
        }
    }

    public function renderFlatBackground(
        SDL $sdl,
        int $viewportX,
        int $viewportY,
        int $viewportWidth,
        int $viewportHeight,
        int $horizonY
    ): void {
        int $screenHorizon = $viewportY + $horizonY;
        int $screenBottom = $viewportY + $viewportHeight - 1;
        int $rightEdge = $viewportX + $viewportWidth - 1;

        // ceiling: dark blue gradient, brighter near horizon
        int $y = $viewportY;
        while ($y < $screenHorizon) {
            int $dy = $screenHorizon - $y;
            // distance from horizon: larger dy = farther ceiling = darker
            int $ceilDist = $dy * 4;
            int $ceilDistSq = $ceilDist * $ceilDist;
            int $ceilFog = 255 - intdiv($ceilDistSq * 200, $ceilDistSq + 360000);
            if ($ceilFog < 20) {
                $ceilFog = 20;
            }
            int $red = intdiv(18 * $ceilFog, 255);
            int $green = intdiv(22 * $ceilFog, 255);
            int $blue = intdiv(68 * $ceilFog, 255);
            $sdl->setDrawColor($red, $green, $blue);
            $sdl->drawLine($viewportX, $y, $rightEdge, $y);
            $y += 1;
        }

        // floor: brown/gray with perspective distance fog per scanline
        $y = $screenHorizon;
        while ($y <= $screenBottom) {
            int $dy = $y - $screenHorizon;
            if ($dy < 1) {
                $dy = 1;
            }
            // perspective floor distance: closer to horizon = farther away
            int $floorDist = intdiv(600 * 8, $dy);
            int $floorDistSq = $floorDist * $floorDist;
            int $floorFog = 255 - intdiv($floorDistSq * 220, $floorDistSq + 640000);
            if ($floorFog < 25) {
                $floorFog = 25;
            }
            int $red = intdiv(72 * $floorFog, 255);
            int $green = intdiv(56 * $floorFog, 255);
            int $blue = intdiv(38 * $floorFog, 255);
            $sdl->setDrawColor($red, $green, $blue);
            $sdl->drawLine($viewportX, $y, $rightEdge, $y);
            $y += 1;
        }
    }

    public function wallLightForSeg(MapData $map, int $segIndex): int {
        int $sectorIndex = $this->frontSectorIndexForSeg($map, $segIndex);
        if ($sectorIndex < 0 || $sectorIndex >= $map->sectorCount) {
            return 160;
        }

        int $light = $map->sectors[$sectorIndex]->light_level;
        if ($light < 0) {
            return 96;
        }
        if ($light > 255) {
            return 255;
        }
        return $light;
    }

    public function shouldRenderSeg(
        MapData $map,
        Camera $camera,
        int $segIndex,
        int $startIndex,
        int $endIndex
    ): bool {
        if ($this->isOneSidedSeg($map, $segIndex)) {
            return true;
        }

        if (!$this->isVisibleSideFacingCamera($map, $camera, $startIndex, $endIndex)) {
            return false;
        }

        int $frontSectorIndex = $this->frontSectorIndexForSeg($map, $segIndex);
        int $backSectorIndex = $this->backSectorIndexForSeg($map, $segIndex);
        if (
            $frontSectorIndex < 0
            || $backSectorIndex < 0
            || $frontSectorIndex >= $map->sectorCount
            || $backSectorIndex >= $map->sectorCount
        ) {
            return false;
        }

        return $map->sectors[$frontSectorIndex]->floor_height !== $map->sectors[$backSectorIndex]->floor_height
            || $map->sectors[$frontSectorIndex]->ceiling_height !== $map->sectors[$backSectorIndex]->ceiling_height;
    }

    public function isVisibleSideFacingCamera(
        MapData $map,
        Camera $camera,
        int $startIndex,
        int $endIndex
    ): bool {
        int $startX = $map->vertexes[$startIndex]->x;
        int $startY = $map->vertexes[$startIndex]->y;
        int $endX = $map->vertexes[$endIndex]->x;
        int $endY = $map->vertexes[$endIndex]->y;
        int $edgeX = $endX - $startX;
        int $edgeY = $endY - $startY;
        int $toCameraX = $camera->x - $startX;
        int $toCameraY = $camera->y - $startY;
        int $cross = ($edgeX * $toCameraY) - ($edgeY * $toCameraX);

        return $cross < 0;
    }

    public function isOneSidedSeg(MapData $map, int $segIndex): bool {
        return $this->backSectorIndexForSeg($map, $segIndex) < 0;
    }

    public function isTwoSidedSeg(MapData $map, int $segIndex): bool {
        return !$this->isOneSidedSeg($map, $segIndex);
    }

    public function frontSectorIndexForSeg(MapData $map, int $segIndex): int {
        return $this->projection->frontSectorIndexForSeg($map, $segIndex);
    }

    public function backSectorIndexForSeg(MapData $map, int $segIndex): int {
        return $this->projection->backSectorIndexForSeg($map, $segIndex);
    }

    public function clampColor(int $value): int {
        if ($value < 0) {
            return 0;
        }
        if ($value > 255) {
            return 255;
        }
        return $value;
    }

    public function absoluteValue(int $value): int {
        if ($value < 0) {
            return -$value;
        }

        return $value;
    }

    public function higherOf(int $left, int $right): int {
        if ($left > $right) {
            return $left;
        }

        return $right;
    }

    public function lowerOf(int $left, int $right): int {
        if ($left < $right) {
            return $left;
        }

        return $right;
    }

    public function drawClippedSpan(
        SDL $sdl,
        int $x,
        int $top,
        int $bottom,
        int $clipTop,
        int $clipBottom,
        int $red,
        int $green,
        int $blue
    ): void {
        if ($top < $clipTop) {
            $top = $clipTop;
        }
        if ($bottom > $clipBottom) {
            $bottom = $clipBottom;
        }
        if ($bottom < $top) {
            return;
        }

        $sdl->setDrawColor($red, $green, $blue);
        $sdl->drawLine($x, $top, $x, $bottom);
    }

    public function drawVerticalSpan(
        SDL $sdl,
        int $x,
        int $top,
        int $bottom,
        int $viewportY,
        int $viewportHeight,
        int $red,
        int $green,
        int $blue
    ): void {
        int $clampedTop = $this->projection->clampScreenY($top, $viewportY, $viewportHeight);
        int $clampedBottom = $this->projection->clampScreenY($bottom, $viewportY, $viewportHeight);
        if ($clampedBottom < $clampedTop) {
            return;
        }

        $sdl->setDrawColor($red, $green, $blue);
        $sdl->drawLine($x, $clampedTop, $x, $clampedBottom);
    }

    public function depthForColumn(
        int $sideA,
        int $depthA,
        int $sideB,
        int $depthB,
        int $screenX,
        int $screenCenter,
        int $focal
    ): int {
        if ($focal <= 0) {
            return $depthA;
        }

        int $ray = intdiv(($screenX - $screenCenter) * 1024, $focal);
        int $deltaSide = $sideB - $sideA;
        int $deltaDepth = $depthB - $depthA;
        int $denominator = (1024 * $deltaSide) - ($ray * $deltaDepth);
        if ($denominator === 0) {
            return $depthA;
        }

        int $numerator = ($ray * $depthA) - (1024 * $sideA);
        int $depth = $depthA + intdiv($deltaDepth * $numerator, $denominator);
        if ($depth <= 0) {
            return $depthA;
        }

        return $depth;
    }

}
