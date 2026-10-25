<?php

namespace Showcases\Doom\Map;

use Showcases\Doom\Data\Linedef;
use Showcases\Doom\Data\Node;
use Showcases\Doom\Data\Sector;
use Showcases\Doom\Data\Seg;
use Showcases\Doom\Data\Sidedef;
use Showcases\Doom\Data\SubSector;
use Showcases\Doom\Data\Thing;
use Showcases\Doom\Data\Vertex;
use Showcases\Doom\Graphics\Flats;
use Showcases\Doom\Graphics\Textures;
use Showcases\Doom\Wad\Wad;

// Reads one map's lumps (THINGS ... BLOCKMAP after the map marker) into packed
// records and resolves everything the game needs per frame: texture and flat
// indices, seg and linedef endpoints, front and back sectors.
final class MapLoader {
    public function load(Wad $wad, Textures $textures, Flats $flats, string $mapName): MapData {
        MapData $map = new MapData($mapName, -1);
        int $marker = $wad->find($mapName);
        if ($marker < 0) {
            return $map;
        }
        int $first = $marker + 1;
        int $end = $marker + 11;
        if ($end > $wad->lumpCount) {
            $end = $wad->lumpCount;
        }
        int $thingsLump = $wad->findKeyBetween($wad->key("THINGS"), $first, $end);
        int $linedefsLump = $wad->findKeyBetween($wad->key("LINEDEFS"), $first, $end);
        int $sidedefsLump = $wad->findKeyBetween($wad->key("SIDEDEFS"), $first, $end);
        int $vertexesLump = $wad->findKeyBetween($wad->key("VERTEXES"), $first, $end);
        int $segsLump = $wad->findKeyBetween($wad->key("SEGS"), $first, $end);
        int $subSectorsLump = $wad->findKeyBetween($wad->key("SSECTORS"), $first, $end);
        int $nodesLump = $wad->findKeyBetween($wad->key("NODES"), $first, $end);
        int $sectorsLump = $wad->findKeyBetween($wad->key("SECTORS"), $first, $end);
        int $blockmapLump = $wad->findKeyBetween($wad->key("BLOCKMAP"), $first, $end);
        if (
            $thingsLump < 0 || $linedefsLump < 0 || $sidedefsLump < 0 || $vertexesLump < 0
            || $segsLump < 0 || $subSectorsLump < 0 || $nodesLump < 0 || $sectorsLump < 0
        ) {
            return $map;
        }

        $map->thingCount = intdiv($wad->lumpSize[$thingsLump], 10);
        $map->linedefCount = intdiv($wad->lumpSize[$linedefsLump], 14);
        $map->sidedefCount = intdiv($wad->lumpSize[$sidedefsLump], 30);
        $map->vertexCount = intdiv($wad->lumpSize[$vertexesLump], 4);
        $map->segCount = intdiv($wad->lumpSize[$segsLump], 12);
        $map->subSectorCount = intdiv($wad->lumpSize[$subSectorsLump], 4);
        $map->nodeCount = intdiv($wad->lumpSize[$nodesLump], 28);
        $map->sectorCount = intdiv($wad->lumpSize[$sectorsLump], 26);
        if ($map->vertexCount <= 0 || $map->segCount <= 0 || $map->sectorCount <= 0 || $map->subSectorCount <= 0) {
            return $map;
        }

        $this->loadVertexes($wad, $map, $wad->lumpOffset[$vertexesLump]);
        $this->loadSectors($wad, $flats, $map, $wad->lumpOffset[$sectorsLump]);
        $this->loadSidedefs($wad, $textures, $map, $wad->lumpOffset[$sidedefsLump]);
        $this->loadLinedefs($wad, $map, $wad->lumpOffset[$linedefsLump]);
        $this->loadSegs($wad, $map, $wad->lumpOffset[$segsLump]);
        $this->loadSubSectors($wad, $map, $wad->lumpOffset[$subSectorsLump]);
        $this->loadNodes($wad, $map, $wad->lumpOffset[$nodesLump]);
        $this->loadThings($wad, $map, $wad->lumpOffset[$thingsLump]);
        if ($blockmapLump >= 0) {
            $this->loadBlockmap($wad, $map, $wad->lumpOffset[$blockmapLump], $wad->lumpSize[$blockmapLump]);
        }
        $map->markerIndex = $marker;
        return $map;
    }

    public function loadVertexes(Wad $wad, MapData $map, int $at): void {
        buffer<Vertex> $vertexes = buffer_new<Vertex>($map->vertexCount);
        for ($i = 0; $i < $map->vertexCount; $i++) {
            int $entry = $i * 4;
            int $base = $at + $entry;
            int $yAt = $base + 2;
            int $x = $wad->i16($base);
            int $y = $wad->i16($yAt);
            $vertexes[$i]->x = $x;
            $vertexes[$i]->y = $y;
            if ($i === 0 || $x < $map->minX) {
                $map->minX = $x;
            }
            if ($i === 0 || $x > $map->maxX) {
                $map->maxX = $x;
            }
            if ($i === 0 || $y < $map->minY) {
                $map->minY = $y;
            }
            if ($i === 0 || $y > $map->maxY) {
                $map->maxY = $y;
            }
        }
        buffer<Vertex> $placeholder = $map->vertexes;
        buffer_free($placeholder);
        $map->vertexes = $vertexes;
    }

    public function loadSectors(Wad $wad, Flats $flats, MapData $map, int $at): void {
        buffer<Sector> $sectors = buffer_new<Sector>($map->sectorCount);
        for ($i = 0; $i < $map->sectorCount; $i++) {
            int $entry = $i * 26;
            int $base = $at + $entry;
            int $ceilingAt = $base + 2;
            int $floorPicAt = $base + 4;
            int $ceilingPicAt = $base + 12;
            int $lightAt = $base + 20;
            int $specialAt = $base + 22;
            int $tagAt = $base + 24;
            $sectors[$i]->floor_height = $wad->i16($base);
            $sectors[$i]->ceiling_height = $wad->i16($ceilingAt);
            $sectors[$i]->floor_pic = $flats->find($wad->keyAt($floorPicAt));
            $sectors[$i]->ceiling_pic = $flats->find($wad->keyAt($ceilingPicAt));
            $sectors[$i]->light_level = $wad->i16($lightAt);
            $sectors[$i]->special = $wad->u16($specialAt);
            $sectors[$i]->tag = $wad->u16($tagAt);
            $sectors[$i]->mover = 0;
        }
        buffer<Sector> $placeholder = $map->sectors;
        buffer_free($placeholder);
        $map->sectors = $sectors;
    }

    public function loadSidedefs(Wad $wad, Textures $textures, MapData $map, int $at): void {
        buffer<Sidedef> $sidedefs = buffer_new<Sidedef>($map->sidedefCount);
        for ($i = 0; $i < $map->sidedefCount; $i++) {
            int $entry = $i * 30;
            int $base = $at + $entry;
            int $yAt = $base + 2;
            int $upperAt = $base + 4;
            int $lowerAt = $base + 12;
            int $middleAt = $base + 20;
            int $sectorAt = $base + 28;
            int $sector = $wad->u16($sectorAt);
            if ($sector >= $map->sectorCount) {
                $sector = 0;
            }
            $sidedefs[$i]->x_offset = $wad->i16($base);
            $sidedefs[$i]->y_offset = $wad->i16($yAt);
            $sidedefs[$i]->upper = $textures->find($wad->keyAt($upperAt));
            $sidedefs[$i]->lower = $textures->find($wad->keyAt($lowerAt));
            $sidedefs[$i]->middle = $textures->find($wad->keyAt($middleAt));
            $sidedefs[$i]->sector = $sector;
        }
        buffer<Sidedef> $placeholder = $map->sidedefs;
        buffer_free($placeholder);
        $map->sidedefs = $sidedefs;
    }

    public function loadLinedefs(Wad $wad, MapData $map, int $at): void {
        buffer<Linedef> $linedefs = buffer_new<Linedef>($map->linedefCount);
        buffer<Vertex> $vertexes = $map->vertexes;
        buffer<Sidedef> $sidedefs = $map->sidedefs;
        for ($i = 0; $i < $map->linedefCount; $i++) {
            int $entry = $i * 14;
            int $base = $at + $entry;
            int $endAt = $base + 2;
            int $flagsAt = $base + 4;
            int $specialAt = $base + 6;
            int $tagAt = $base + 8;
            int $rightAt = $base + 10;
            int $leftAt = $base + 12;
            int $start = $wad->u16($base);
            int $end = $wad->u16($endAt);
            if ($start >= $map->vertexCount) {
                $start = 0;
            }
            if ($end >= $map->vertexCount) {
                $end = 0;
            }
            int $right = $wad->i16($rightAt);
            int $left = $wad->i16($leftAt);
            if ($right >= $map->sidedefCount) {
                $right = -1;
            }
            if ($left >= $map->sidedefCount) {
                $left = -1;
            }
            $linedefs[$i]->mapped = 0;
            $linedefs[$i]->start_vertex = $start;
            $linedefs[$i]->end_vertex = $end;
            $linedefs[$i]->flags = $wad->u16($flagsAt);
            $linedefs[$i]->special = $wad->u16($specialAt);
            $linedefs[$i]->tag = $wad->u16($tagAt);
            $linedefs[$i]->right_side = $right;
            $linedefs[$i]->left_side = $left;
            $linedefs[$i]->x1 = $vertexes[$start]->x;
            $linedefs[$i]->y1 = $vertexes[$start]->y;
            $linedefs[$i]->x2 = $vertexes[$end]->x;
            $linedefs[$i]->y2 = $vertexes[$end]->y;
            $linedefs[$i]->front_sector = -1;
            $linedefs[$i]->back_sector = -1;
            if ($right >= 0) {
                $linedefs[$i]->front_sector = $sidedefs[$right]->sector;
            }
            if ($left >= 0) {
                $linedefs[$i]->back_sector = $sidedefs[$left]->sector;
            }
        }
        buffer<Linedef> $placeholder = $map->linedefs;
        buffer_free($placeholder);
        $map->linedefs = $linedefs;
    }

    public function loadSegs(Wad $wad, MapData $map, int $at): void {
        buffer<Seg> $segs = buffer_new<Seg>($map->segCount);
        buffer<Vertex> $vertexes = $map->vertexes;
        buffer<Linedef> $linedefs = $map->linedefs;
        buffer<Sidedef> $sidedefs = $map->sidedefs;
        for ($i = 0; $i < $map->segCount; $i++) {
            int $entry = $i * 12;
            int $base = $at + $entry;
            int $endAt = $base + 2;
            int $angleAt = $base + 4;
            int $linedefAt = $base + 6;
            int $directionAt = $base + 8;
            int $offsetAt = $base + 10;
            int $start = $wad->u16($base);
            int $end = $wad->u16($endAt);
            int $linedef = $wad->u16($linedefAt);
            int $direction = $wad->u16($directionAt);
            if ($start >= $map->vertexCount) {
                $start = 0;
            }
            if ($end >= $map->vertexCount) {
                $end = 0;
            }
            if ($linedef >= $map->linedefCount) {
                $linedef = 0;
            }
            int $x1 = $vertexes[$start]->x;
            int $y1 = $vertexes[$start]->y;
            int $x2 = $vertexes[$end]->x;
            int $y2 = $vertexes[$end]->y;
            $segs[$i]->start_vertex = $start;
            $segs[$i]->end_vertex = $end;
            $segs[$i]->angle = $wad->u16($angleAt);
            $segs[$i]->linedef = $linedef;
            $segs[$i]->direction = $direction;
            $segs[$i]->offset = $wad->i16($offsetAt);
            $segs[$i]->x1 = $x1;
            $segs[$i]->y1 = $y1;
            $segs[$i]->x2 = $x2;
            $segs[$i]->y2 = $y2;

            // the seg's own side of its linedef is its front
            int $frontSide = $linedefs[$linedef]->right_side;
            int $backSide = $linedefs[$linedef]->left_side;
            if ($direction !== 0) {
                $frontSide = $linedefs[$linedef]->left_side;
                $backSide = $linedefs[$linedef]->right_side;
            }
            $segs[$i]->sidedef = $frontSide;
            $segs[$i]->front_sector = -1;
            $segs[$i]->back_sector = -1;
            if ($frontSide >= 0) {
                $segs[$i]->front_sector = $sidedefs[$frontSide]->sector;
            }
            if ($backSide >= 0) {
                $segs[$i]->back_sector = $sidedefs[$backSide]->sector;
            }
            int $dx = $x2 - $x1;
            int $dy = $y2 - $y1;
            int $dxSquared = $dx * $dx;
            int $dySquared = $dy * $dy;
            int $lengthSquared = $dxSquared + $dySquared;
            $segs[$i]->length = (int)round(sqrt($lengthSquared));
        }
        buffer<Seg> $placeholder = $map->segs;
        buffer_free($placeholder);
        $map->segs = $segs;
    }

    public function loadSubSectors(Wad $wad, MapData $map, int $at): void {
        buffer<SubSector> $subSectors = buffer_new<SubSector>($map->subSectorCount);
        buffer<Seg> $segs = $map->segs;
        for ($i = 0; $i < $map->subSectorCount; $i++) {
            int $entry = $i * 4;
            int $base = $at + $entry;
            int $firstAt = $base + 2;
            int $first = $wad->u16($firstAt);
            $subSectors[$i]->seg_count = $wad->u16($base);
            $subSectors[$i]->first_seg = $first;
            $subSectors[$i]->sector = 0;
            if ($first < $map->segCount && $segs[$first]->front_sector >= 0) {
                $subSectors[$i]->sector = $segs[$first]->front_sector;
            }
        }
        buffer<SubSector> $placeholder = $map->subSectors;
        buffer_free($placeholder);
        $map->subSectors = $subSectors;
    }

    public function loadNodes(Wad $wad, MapData $map, int $at): void {
        int $count = $map->nodeCount;
        if ($count <= 0) {
            $count = 1;
        }
        buffer<Node> $nodes = buffer_new<Node>($count);
        for ($i = 0; $i < $map->nodeCount; $i++) {
            int $entry = $i * 28;
            int $base = $at + $entry;
            int $yAt = $base + 2;
            int $dxAt = $base + 4;
            int $dyAt = $base + 6;
            int $rightAt = $base + 24;
            int $leftAt = $base + 26;
            $nodes[$i]->x = $wad->i16($base);
            $nodes[$i]->y = $wad->i16($yAt);
            $nodes[$i]->dx = $wad->i16($dxAt);
            $nodes[$i]->dy = $wad->i16($dyAt);
            $nodes[$i]->right_child = $wad->u16($rightAt);
            $nodes[$i]->left_child = $wad->u16($leftAt);
        }
        buffer<Node> $placeholder = $map->nodes;
        buffer_free($placeholder);
        $map->nodes = $nodes;
    }

    public function loadThings(Wad $wad, MapData $map, int $at): void {
        int $count = $map->thingCount;
        if ($count <= 0) {
            $count = 1;
        }
        buffer<Thing> $things = buffer_new<Thing>($count);
        bool $foundStart = false;
        for ($i = 0; $i < $map->thingCount; $i++) {
            int $entry = $i * 10;
            int $base = $at + $entry;
            int $yAt = $base + 2;
            int $angleAt = $base + 4;
            int $typeAt = $base + 6;
            int $flagsAt = $base + 8;
            int $type = $wad->u16($typeAt);
            $things[$i]->x = $wad->i16($base);
            $things[$i]->y = $wad->i16($yAt);
            $things[$i]->angle = $wad->i16($angleAt);
            $things[$i]->type = $type;
            $things[$i]->flags = $wad->u16($flagsAt);
            if ($type === 1 && !$foundStart) {
                $map->playerStartX = $things[$i]->x;
                $map->playerStartY = $things[$i]->y;
                $map->playerStartAngle = $things[$i]->angle;
                $foundStart = true;
            }
        }
        buffer<Thing> $placeholder = $map->things;
        buffer_free($placeholder);
        $map->things = $things;
    }

    // Keeps the lump's 16-bit words; each cell points at its list, minus the leading 0 marker.
    public function loadBlockmap(Wad $wad, MapData $map, int $at, int $size): void {
        int $words = intdiv($size, 2);
        if ($words < 4) {
            return;
        }
        buffer<int> $list = buffer_new<int>($words);
        for ($w = 0; $w < $words; $w++) {
            int $byteOffset = $w * 2;
            int $wordAt = $at + $byteOffset;
            int $value = $wad->u16($wordAt);
            if ($value === 65535) {
                $value = -1;
            }
            $list[$w] = $value;
        }
        int $originX = $wad->i16($at);
        int $originYAt = $at + 2;
        int $columnsAt = $at + 4;
        int $rowsAt = $at + 6;
        int $columns = $list[2];
        int $rows = $list[3];
        int $cells = $columns * $rows;
        int $headerWords = $cells + 4;
        if ($cells <= 0 || $headerWords > $words) {
            buffer_free($list);
            return;
        }
        buffer<int> $cell = buffer_new<int>($cells);
        for ($c = 0; $c < $cells; $c++) {
            int $pointerAt = $c + 4;
            int $start = $list[$pointerAt];
            if ($start < $words && $list[$start] === 0) {
                int $afterMarker = $start + 1;
                $start = $afterMarker;
            }
            $cell[$c] = $start;
        }
        $map->blockOriginX = $originX;
        $map->blockOriginY = $wad->i16($originYAt);
        $map->blockColumns = $wad->u16($columnsAt);
        $map->blockRows = $wad->u16($rowsAt);
        buffer<int> $oldCell = $map->blockCell;
        buffer_free($oldCell);
        buffer<int> $oldList = $map->blockList;
        buffer_free($oldList);
        $map->blockCell = $cell;
        $map->blockList = $list;
    }
}
