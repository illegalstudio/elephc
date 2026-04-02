<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\Map\MapData;

class Projection {
    public function eyeLevelOffset(): int {
        return 41;
    }

    public function cameraEyeZ(MapData $map, int $cameraSubSector): int {
        return $this->floorHeightForSubSector($map, $cameraSubSector) + $this->eyeLevelOffset();
    }

    public function floorHeightForSubSector(MapData $map, int $subSectorIndex): int {
        int $sectorIndex = $this->sectorIndexForSubSector($map, $subSectorIndex);
        if ($sectorIndex < 0 || $sectorIndex >= $map->sectorCount) {
            return 0;
        }

        return $map->sectors[$sectorIndex]->floor_height;
    }

    public function sectorIndexForSubSector(MapData $map, int $subSectorIndex): int {
        if ($subSectorIndex < 0 || $subSectorIndex >= $map->subSectorCount) {
            return -1;
        }

        int $firstSeg = $map->subSectors[$subSectorIndex]->first_seg_index;
        if ($firstSeg < 0 || $firstSeg >= $map->segCount) {
            return -1;
        }

        return $this->frontSectorIndexForSeg($map, $firstSeg);
    }

    public function frontSectorIndexForSeg(MapData $map, int $segIndex): int {
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

    public function backSectorIndexForSeg(MapData $map, int $segIndex): int {
        int $linedefIndex = $map->segs[$segIndex]->linedef_index;
        if ($linedefIndex < 0 || $linedefIndex >= $map->linedefCount) {
            return -1;
        }

        int $sidedefIndex = $map->linedefs[$linedefIndex]->left_sidedef;
        if ($map->segs[$segIndex]->direction != 0) {
            $sidedefIndex = $map->linedefs[$linedefIndex]->right_sidedef;
        }
        if ($sidedefIndex < 0 || $sidedefIndex >= $map->sidedefCount) {
            return -1;
        }

        return $map->sidedefs[$sidedefIndex]->sector_index;
    }

    public function projectScreenY(
        int $worldZ,
        int $eyeZ,
        int $depth,
        int $viewportY,
        int $horizonY,
        int $focal
    ): int {
        if ($depth <= 0) {
            $depth = 1;
        }

        return $viewportY + $horizonY - intdiv(($worldZ - $eyeZ) * $focal, $depth);
    }

    public function clampScreenY(int $value, int $viewportY, int $viewportHeight): int {
        if ($value < $viewportY) {
            return $viewportY;
        }
        if ($value >= $viewportY + $viewportHeight) {
            return $viewportY + $viewportHeight - 1;
        }

        return $value;
    }
}
