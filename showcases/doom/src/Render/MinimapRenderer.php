<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\App\Config;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Player\Camera;
use Showcases\Doom\SDL\SDL;

class MinimapRenderer {
    public function render(SDL $sdl, Config $config, MapData $map, Camera $camera, $subSectorOrder): void {
        $this->renderViewport(
            $sdl,
            $map,
            $camera,
            $subSectorOrder,
            60,
            60,
            $config->windowWidth - 120,
            $config->windowHeight - 120
        );
    }

    public function renderInset(SDL $sdl, Config $config, MapData $map, Camera $camera, $subSectorOrder): void {
        $this->renderViewport(
            $sdl,
            $map,
            $camera,
            $subSectorOrder,
            $config->windowWidth - 300,
            20,
            260,
            200
        );
    }

    public function renderViewport(
        SDL $sdl,
        MapData $map,
        Camera $camera,
        $subSectorOrder,
        int $originX,
        int $originY,
        int $drawWidth,
        int $drawHeight
    ): void {
        if (!$map->isValid() || !$map->hasBounds() || $map->vertexCount <= 0 || $map->linedefCount <= 0) {
            return;
        }

        int $worldWidth = $map->maxX - $map->minX;
        int $worldHeight = $map->maxY - $map->minY;
        if ($worldWidth <= 0) {
            $worldWidth = 1;
        }
        if ($worldHeight <= 0) {
            $worldHeight = 1;
        }

        int $scaleX = intdiv($drawWidth * 1024, $worldWidth);
        int $scaleY = intdiv($drawHeight * 1024, $worldHeight);
        int $scale = $scaleX;
        if ($scaleY < $scale) {
            $scale = $scaleY;
        }
        if ($scale <= 0) {
            $scale = 1;
        }

        int $projectedWidth = intdiv($worldWidth * $scale, 1024);
        int $projectedHeight = intdiv($worldHeight * $scale, 1024);
        int $padX = intdiv($drawWidth - $projectedWidth, 2);
        int $padY = intdiv($drawHeight - $projectedHeight, 2);

        $sdl->setDrawColor(92, 184, 92);
        int $i = 0;
        while ($i < $map->linedefCount) {
            int $startIndex = $map->linedefs[$i]->start_vertex;
            int $endIndex = $map->linedefs[$i]->end_vertex;
            if (
                $startIndex >= 0
                && $endIndex >= 0
                && $startIndex < $map->vertexCount
                && $endIndex < $map->vertexCount
            ) {
                int $x1 = $this->projectX($map->vertexes[$startIndex]->x, $map, $originX, $padX, $scale);
                int $y1 = $this->projectY($map->vertexes[$startIndex]->y, $map, $originY, $padY, $drawHeight, $scale);
                int $x2 = $this->projectX($map->vertexes[$endIndex]->x, $map, $originX, $padX, $scale);
                int $y2 = $this->projectY($map->vertexes[$endIndex]->y, $map, $originY, $padY, $drawHeight, $scale);
                $sdl->drawLine($x1, $y1, $x2, $y2);
            }
            $i += 1;
        }

        $this->renderSubSectorOverlay(
            $sdl,
            $map,
            $subSectorOrder,
            $originX,
            $originY,
            $padX,
            $padY,
            $drawHeight,
            $scale
        );

        int $playerX = $this->projectX($camera->x, $map, $originX, $padX, $scale);
        int $playerY = $this->projectY($camera->y, $map, $originY, $padY, $drawHeight, $scale);
        $sdl->setDrawColor(255, 214, 102);
        $this->drawCross($sdl, $playerX, $playerY);
        $this->drawHeading($sdl, $playerX, $playerY, $camera->angle);
    }

    public function projectX(
        int $worldX,
        MapData $map,
        int $originX,
        int $padX,
        int $scale
    ): int {
        return $originX + $padX + intdiv(($worldX - $map->minX) * $scale, 1024);
    }

    public function projectY(
        int $worldY,
        MapData $map,
        int $originY,
        int $padY,
        int $drawHeight,
        int $scale
    ): int {
        int $relative = intdiv(($worldY - $map->minY) * $scale, 1024);
        return $originY + $padY + ($drawHeight - $relative);
    }

    public function drawCross(SDL $sdl, int $x, int $y): void {
        $sdl->drawPoint($x, $y);
        $sdl->drawPoint($x - 1, $y);
        $sdl->drawPoint($x + 1, $y);
        $sdl->drawPoint($x, $y - 1);
        $sdl->drawPoint($x, $y + 1);
    }

    public function renderSubSectorOverlay(
        SDL $sdl,
        MapData $map,
        $subSectorOrder,
        int $originX,
        int $originY,
        int $padX,
        int $padY,
        int $drawHeight,
        int $scale
    ): void {
        int $count = count($subSectorOrder);
        if ($count <= 0) {
            return;
        }

        int $i = 0;
        while ($i < $count) {
            int $subSectorIndex = $subSectorOrder[$i];
            if ($subSectorIndex >= 0 && $subSectorIndex < $map->subSectorCount) {
                int $intensity = 220 - intdiv($i * 140, $count);
                if ($intensity < 80) {
                    $intensity = 80;
                }
                $sdl->setDrawColor(80, $intensity, 220);
                $this->renderSubSector(
                    $sdl,
                    $map,
                    $subSectorIndex,
                    $originX,
                    $originY,
                    $padX,
                    $padY,
                    $drawHeight,
                    $scale
                );
            }
            $i += 1;
        }
    }

    public function renderSubSector(
        SDL $sdl,
        MapData $map,
        int $subSectorIndex,
        int $originX,
        int $originY,
        int $padX,
        int $padY,
        int $drawHeight,
        int $scale
    ): void {
        int $firstSeg = $map->subSectors[$subSectorIndex]->first_seg_index;
        int $segCount = $map->subSectors[$subSectorIndex]->seg_count;
        int $i = 0;

        while ($i < $segCount) {
            int $segIndex = $firstSeg + $i;
            if ($segIndex >= 0 && $segIndex < $map->segCount) {
                int $startIndex = $map->segs[$segIndex]->start_vertex;
                int $endIndex = $map->segs[$segIndex]->end_vertex;
                if (
                    $startIndex >= 0
                    && $endIndex >= 0
                    && $startIndex < $map->vertexCount
                    && $endIndex < $map->vertexCount
                ) {
                    int $x1 = $this->projectX($map->vertexes[$startIndex]->x, $map, $originX, $padX, $scale);
                    int $y1 = $this->projectY($map->vertexes[$startIndex]->y, $map, $originY, $padY, $drawHeight, $scale);
                    int $x2 = $this->projectX($map->vertexes[$endIndex]->x, $map, $originX, $padX, $scale);
                    int $y2 = $this->projectY($map->vertexes[$endIndex]->y, $map, $originY, $padY, $drawHeight, $scale);
                    $sdl->drawLine($x1, $y1, $x2, $y2);
                }
            }
            $i += 1;
        }
    }

    public function drawHeading(SDL $sdl, int $x, int $y, int $angle): void {
        int $dx = 0;
        int $dy = -8;

        if ($angle >= 23 && $angle < 68) {
            $dx = 6;
            $dy = -6;
        } else if ($angle >= 68 && $angle < 113) {
            $dx = 8;
            $dy = 0;
        } else if ($angle >= 113 && $angle < 158) {
            $dx = 6;
            $dy = 6;
        } else if ($angle >= 158 && $angle < 203) {
            $dx = 0;
            $dy = 8;
        } else if ($angle >= 203 && $angle < 248) {
            $dx = -6;
            $dy = 6;
        } else if ($angle >= 248 && $angle < 293) {
            $dx = -8;
            $dy = 0;
        } else if ($angle >= 293 && $angle < 338) {
            $dx = -6;
            $dy = -6;
        }

        $sdl->drawLine($x, $y, $x + $dx, $y + $dy);
    }
}
