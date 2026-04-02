<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\App\Config;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Player\Camera;
use Showcases\Doom\SDL\SDL;

class WallRenderer {
    public function render(SDL $sdl, Config $config, MapData $map, Camera $camera, $subSectorOrder): void {
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

        int $subSectorCount = count($subSectorOrder);
        int $orderIndex = $subSectorCount - 1;
        while ($orderIndex >= 0) {
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
                            $nearPlane
                        );
                    }
                    $segOffset += 1;
                }
            }
            $orderIndex -= 1;
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
        int $nearPlane
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

        int $worldX1 = $map->vertexes[$startIndex]->x;
        int $worldY1 = $map->vertexes[$startIndex]->y;
        int $worldX2 = $map->vertexes[$endIndex]->x;
        int $worldY2 = $map->vertexes[$endIndex]->y;

        int $relX1 = $worldX1 - $camera->x;
        int $relY1 = $worldY1 - $camera->y;
        int $relX2 = $worldX2 - $camera->x;
        int $relY2 = $worldY2 - $camera->y;

        int $forwardX = $this->directionUnitX($camera->angle);
        int $forwardY = $this->directionUnitY($camera->angle);
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
            $side1 = $side1 + intdiv(($side2 - $side1) * $num, $den);
            $depth1 = $nearPlane;
        }
        if ($depth2 <= $nearPlane) {
            int $den = $depth1 - $depth2;
            if ($den <= 0) {
                return;
            }
            int $num = $nearPlane - $depth2;
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
        int $leftDepth = $depth1;
        int $rightDepth = $depth2;
        if ($leftX > $rightScreenX) {
            $leftX = $screenX2;
            $rightScreenX = $screenX1;
            $leftDepth = $depth2;
            $rightDepth = $depth1;
        }

        if ($rightScreenX < $viewportX || $leftX >= $viewportX + $viewportWidth) {
            return;
        }

        int $height = $this->wallHeightForSeg($map, $segIndex);
        if ($height <= 0) {
            $height = 96;
        }

        int $light = $this->wallLightForSeg($map, $segIndex);
        int $baseRed = 40 + intdiv($light * 120, 255);
        int $baseGreen = 60 + intdiv($light * 140, 255);
        int $baseBlue = 80 + intdiv($light * 110, 255);
        if ($map->segs[$segIndex]->direction != 0) {
            $baseRed += 18;
            $baseGreen += 12;
        }
        if ($baseRed > 255) {
            $baseRed = 255;
        }
        if ($baseGreen > 255) {
            $baseGreen = 255;
        }
        if ($baseBlue > 255) {
            $baseBlue = 255;
        }

        if ($leftX < $viewportX) {
            $leftX = $viewportX;
        }
        if ($rightScreenX >= $viewportX + $viewportWidth) {
            $rightScreenX = ($viewportX + $viewportWidth) - 1;
        }

        int $x = $leftX;
        while ($x <= $rightScreenX) {
            int $span = $rightScreenX - $leftX;
            int $depth = $leftDepth;
            if ($span > 0) {
                $depth = $leftDepth + intdiv(($rightDepth - $leftDepth) * ($x - $leftX), $span);
            }
            if ($depth < $nearPlane) {
                $depth = $nearPlane;
            }

            int $projectedHeight = intdiv($height * $focal, $depth);
            int $top = $viewportY + $horizonY - intdiv($projectedHeight, 2);
            int $bottom = $viewportY + $horizonY + intdiv($projectedHeight, 2);

            if ($top < $viewportY) {
                $top = $viewportY;
            }
            if ($bottom >= $viewportY + ($horizonY * 2)) {
                $bottom = $viewportY + ($horizonY * 2) - 1;
            }

            $sdl->setDrawColor($baseRed, $baseGreen, $baseBlue);
            $sdl->drawLine($x, $top, $x, $bottom);
            $x += 1;
        }
    }

    public function wallHeightForSeg(MapData $map, int $segIndex): int {
        int $sectorIndex = $this->sectorIndexForSeg($map, $segIndex);
        if ($sectorIndex < 0 || $sectorIndex >= $map->sectorCount) {
            return 96;
        }

        return $map->sectors[$sectorIndex]->ceiling_height - $map->sectors[$sectorIndex]->floor_height;
    }

    public function wallLightForSeg(MapData $map, int $segIndex): int {
        int $sectorIndex = $this->sectorIndexForSeg($map, $segIndex);
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

    public function sectorIndexForSeg(MapData $map, int $segIndex): int {
        int $linedefIndex = $map->segs[$segIndex]->linedef_index;
        if ($linedefIndex < 0 || $linedefIndex >= $map->linedefCount) {
            return -1;
        }

        int $sidedefIndex = $map->linedefs[$linedefIndex]->right_sidedef;
        if ($map->segs[$segIndex]->direction != 0) {
            $sidedefIndex = $map->linedefs[$linedefIndex]->left_sidedef;
        }
        if ($sidedefIndex < 0 || $sidedefIndex >= $map->sidedefCount) {
            return -1;
        }

        return $map->sidedefs[$sidedefIndex]->sector_index;
    }

    public function directionBucket16(int $angle): int {
        int $adjusted = $angle + 11;
        if ($adjusted >= 360) {
            $adjusted = $adjusted - 360;
        }

        return intdiv($adjusted * 16, 360);
    }

    public function directionUnitX(int $angle): int {
        int $bucket = $this->directionBucket16($angle);

        if ($bucket === 1) {
            return 392;
        }
        if ($bucket === 2) {
            return 724;
        }
        if ($bucket === 3) {
            return 946;
        }
        if ($bucket === 4) {
            return 1024;
        }
        if ($bucket === 5) {
            return 946;
        }
        if ($bucket === 6) {
            return 724;
        }
        if ($bucket === 7) {
            return 392;
        }
        if ($bucket === 9) {
            return -392;
        }
        if ($bucket === 10) {
            return -724;
        }
        if ($bucket === 11) {
            return -946;
        }
        if ($bucket === 12) {
            return -1024;
        }
        if ($bucket === 13) {
            return -946;
        }
        if ($bucket === 14) {
            return -724;
        }
        if ($bucket === 15) {
            return -392;
        }

        return 0;
    }

    public function directionUnitY(int $angle): int {
        int $bucket = $this->directionBucket16($angle);

        if ($bucket === 0) {
            return -1024;
        }
        if ($bucket === 1) {
            return -946;
        }
        if ($bucket === 2) {
            return -724;
        }
        if ($bucket === 3) {
            return -392;
        }
        if ($bucket === 5) {
            return 392;
        }
        if ($bucket === 6) {
            return 724;
        }
        if ($bucket === 7) {
            return 946;
        }
        if ($bucket === 8) {
            return 1024;
        }
        if ($bucket === 9) {
            return 946;
        }
        if ($bucket === 10) {
            return 724;
        }
        if ($bucket === 11) {
            return 392;
        }
        if ($bucket === 13) {
            return -392;
        }
        if ($bucket === 14) {
            return -724;
        }
        if ($bucket === 15) {
            return -946;
        }

        return 0;
    }
}
