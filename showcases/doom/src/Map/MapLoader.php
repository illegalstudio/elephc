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
use Showcases\Doom\IO\BinaryReader;
use Showcases\Doom\Wad\WadEntry;
use Showcases\Doom\Wad\WadFile;

class MapLoader {
    public function load(WadFile $wad, string $mapName): MapData {
        MapData $map = new MapData($mapName, -1);
        if (!$wad->isValid()) {
            return $map;
        }

        string $bytes = file_get_contents($wad->path);
        BinaryReader $reader = new BinaryReader($bytes);

        int $markerIndex = $this->findEntryIndex($reader, $wad, $mapName);
        if ($markerIndex < 0) {
            return $map;
        }

        $thingsEntry = $this->findMapEntry($reader, $wad, $markerIndex, "THINGS");
        $linedefsEntry = $this->findMapEntry($reader, $wad, $markerIndex, "LINEDEFS");
        $sidedefsEntry = $this->findMapEntry($reader, $wad, $markerIndex, "SIDEDEFS");
        $vertexesEntry = $this->findMapEntry($reader, $wad, $markerIndex, "VERTEXES");
        $segsEntry = $this->findMapEntry($reader, $wad, $markerIndex, "SEGS");
        $subSectorsEntry = $this->findMapEntry($reader, $wad, $markerIndex, "SSECTORS");
        $nodesEntry = $this->findMapEntry($reader, $wad, $markerIndex, "NODES");
        $sectorsEntry = $this->findMapEntry($reader, $wad, $markerIndex, "SECTORS");

        if (
            $thingsEntry->name === ""
            || $linedefsEntry->name === ""
            || $sidedefsEntry->name === ""
            || $vertexesEntry->name === ""
            || $segsEntry->name === ""
            || $subSectorsEntry->name === ""
            || $nodesEntry->name === ""
            || $sectorsEntry->name === ""
        ) {
            return $map;
        }

        $map->mapName = $mapName;
        $map->markerIndex = $markerIndex;

        $map->thingCount = $this->entryCount($thingsEntry, 10);
        $map->linedefCount = $this->entryCount($linedefsEntry, 14);
        $map->sidedefCount = $this->entryCount($sidedefsEntry, 30);
        $map->vertexCount = $this->entryCount($vertexesEntry, 4);
        $map->segCount = $this->entryCount($segsEntry, 12);
        $map->subSectorCount = $this->entryCount($subSectorsEntry, 4);
        $map->nodeCount = $this->entryCount($nodesEntry, 28);
        $map->sectorCount = $this->entryCount($sectorsEntry, 26);

        buffer<Thing> $things = buffer_new<Thing>($map->thingCount);
        buffer<Linedef> $linedefs = buffer_new<Linedef>($map->linedefCount);
        buffer<Sidedef> $sidedefs = buffer_new<Sidedef>($map->sidedefCount);
        buffer<Vertex> $vertexes = buffer_new<Vertex>($map->vertexCount);
        buffer<Seg> $segs = buffer_new<Seg>($map->segCount);
        buffer<SubSector> $subSectors = buffer_new<SubSector>($map->subSectorCount);
        buffer<Node> $nodes = buffer_new<Node>($map->nodeCount);
        buffer<Sector> $sectors = buffer_new<Sector>($map->sectorCount);

        int $i = 0;
        while ($i < $map->vertexCount) {
            int $base = $vertexesEntry->offset + ($i * 4);
            int $x = $reader->readI16LE($base);
            int $y = $reader->readI16LE($base + 2);
            $vertexes[$i]->x = $x;
            $vertexes[$i]->y = $y;

            if ($i === 0) {
                $map->minX = $x;
                $map->maxX = $x;
                $map->minY = $y;
                $map->maxY = $y;
            } else {
                if ($x < $map->minX) {
                    $map->minX = $x;
                }
                if ($x > $map->maxX) {
                    $map->maxX = $x;
                }
                if ($y < $map->minY) {
                    $map->minY = $y;
                }
                if ($y > $map->maxY) {
                    $map->maxY = $y;
                }
            }

            $i += 1;
        }

        $i = 0;
        while ($i < $map->thingCount) {
            int $base = $thingsEntry->offset + ($i * 10);
            $things[$i]->x = $reader->readI16LE($base);
            $things[$i]->y = $reader->readI16LE($base + 2);
            $things[$i]->angle = $reader->readU16LE($base + 4);
            $things[$i]->type = $reader->readU16LE($base + 6);
            $things[$i]->flags = $reader->readU16LE($base + 8);

            if ($i === 0 || $things[$i]->type === 1) {
                $map->playerStartX = $things[$i]->x;
                $map->playerStartY = $things[$i]->y;
                $map->playerStartAngle = $things[$i]->angle;
                if ($things[$i]->type === 1) {
                    $i = $map->thingCount;
                    continue;
                }
            }

            $i += 1;
        }

        $i = 0;
        while ($i < $map->linedefCount) {
            int $base = $linedefsEntry->offset + ($i * 14);
            $linedefs[$i]->start_vertex = $reader->readU16LE($base);
            $linedefs[$i]->end_vertex = $reader->readU16LE($base + 2);
            $linedefs[$i]->flags = $reader->readU16LE($base + 4);
            $linedefs[$i]->special_type = $reader->readU16LE($base + 6);
            $linedefs[$i]->sector_tag = $reader->readU16LE($base + 8);
            $linedefs[$i]->right_sidedef = $reader->readI16LE($base + 10);
            $linedefs[$i]->left_sidedef = $reader->readI16LE($base + 12);
            $i += 1;
        }

        $i = 0;
        while ($i < $map->sidedefCount) {
            int $base = $sidedefsEntry->offset + ($i * 30);
            $sidedefs[$i]->offset_x = $reader->readI16LE($base);
            $sidedefs[$i]->offset_y = $reader->readI16LE($base + 2);
            $sidedefs[$i]->sector_index = $reader->readU16LE($base + 28);
            $i += 1;
        }

        $i = 0;
        while ($i < $map->segCount) {
            int $base = $segsEntry->offset + ($i * 12);
            $segs[$i]->start_vertex = $reader->readU16LE($base);
            $segs[$i]->end_vertex = $reader->readU16LE($base + 2);
            $segs[$i]->angle = $reader->readU16LE($base + 4);
            $segs[$i]->linedef_index = $reader->readU16LE($base + 6);
            $segs[$i]->direction = $reader->readU16LE($base + 8);
            $segs[$i]->offset = $reader->readU16LE($base + 10);
            $i += 1;
        }

        $i = 0;
        while ($i < $map->subSectorCount) {
            int $base = $subSectorsEntry->offset + ($i * 4);
            $subSectors[$i]->seg_count = $reader->readU16LE($base);
            $subSectors[$i]->first_seg_index = $reader->readU16LE($base + 2);
            $i += 1;
        }

        $i = 0;
        while ($i < $map->nodeCount) {
            int $base = $nodesEntry->offset + ($i * 28);
            $nodes[$i]->partition_x = $reader->readI16LE($base);
            $nodes[$i]->partition_y = $reader->readI16LE($base + 2);
            $nodes[$i]->delta_x = $reader->readI16LE($base + 4);
            $nodes[$i]->delta_y = $reader->readI16LE($base + 6);
            $nodes[$i]->right_top = $reader->readI16LE($base + 8);
            $nodes[$i]->right_bottom = $reader->readI16LE($base + 10);
            $nodes[$i]->right_left = $reader->readI16LE($base + 12);
            $nodes[$i]->right_right = $reader->readI16LE($base + 14);
            $nodes[$i]->left_top = $reader->readI16LE($base + 16);
            $nodes[$i]->left_bottom = $reader->readI16LE($base + 18);
            $nodes[$i]->left_left = $reader->readI16LE($base + 20);
            $nodes[$i]->left_right = $reader->readI16LE($base + 22);
            $nodes[$i]->right_child = $reader->readU16LE($base + 24);
            $nodes[$i]->left_child = $reader->readU16LE($base + 26);
            $i += 1;
        }

        $i = 0;
        while ($i < $map->sectorCount) {
            int $base = $sectorsEntry->offset + ($i * 26);
            $sectors[$i]->floor_height = $reader->readI16LE($base);
            $sectors[$i]->ceiling_height = $reader->readI16LE($base + 2);
            $sectors[$i]->light_level = $reader->readI16LE($base + 20);
            $sectors[$i]->special_type = $reader->readU16LE($base + 22);
            $sectors[$i]->tag = $reader->readU16LE($base + 24);
            $i += 1;
        }

        $map->things = $things;
        $map->linedefs = $linedefs;
        $map->sidedefs = $sidedefs;
        $map->vertexes = $vertexes;
        $map->segs = $segs;
        $map->subSectors = $subSectors;
        $map->nodes = $nodes;
        $map->sectors = $sectors;

        // load PLAYPAL palette (256 RGB triplets = 768 bytes)
        int $palIdx = $this->findEntryIndex($reader, $wad, "PLAYPAL");
        if ($palIdx >= 0) {
            $palEntry = $this->readDirectoryEntry($reader, $wad, $palIdx);
            if ($palEntry->size >= 768) {
                $palR = [];
                $palG = [];
                $palB = [];
                int $pi = 0;
                while ($pi < 256) {
                    int $po = $palEntry->offset + ($pi * 3);
                    $palR[] = $reader->readU8($po);
                    $palG[] = $reader->readU8($po + 1);
                    $palB[] = $reader->readU8($po + 2);
                    $pi += 1;
                }
                $map->paletteReds = $palR;
                $map->paletteGreens = $palG;
                $map->paletteBlues = $palB;
                $map->paletteCount = 256;
            }
        }

        return $map;
    }

    public function findMapEntry(
        BinaryReader $reader,
        WadFile $wad,
        int $markerIndex,
        string $entryName
    ): WadEntry {
        int $i = $markerIndex + 1;
        int $limit = $markerIndex + 11;

        while ($i < $wad->entryCount && $i <= $limit) {
            WadEntry $entry = $this->readDirectoryEntry($reader, $wad, $i);
            if ($entry->name === $entryName) {
                return $entry;
            }
            $i += 1;
        }

        return new WadEntry("", 0, 0);
    }

    public function findEntryIndex(BinaryReader $reader, WadFile $wad, string $name): int {
        int $i = 0;

        while ($i < $wad->entryCount) {
            WadEntry $entry = $this->readDirectoryEntry($reader, $wad, $i);
            if ($entry->name === $name) {
                return $i;
            }
            $i += 1;
        }

        return -1;
    }

    public function readDirectoryEntry(BinaryReader $reader, WadFile $wad, int $index): WadEntry {
        if ($index < 0 || $index >= $wad->entryCount) {
            return new WadEntry("", 0, 0);
        }

        int $entryOffset = $wad->directoryOffset + ($index * 16);
        return new WadEntry(
            $reader->readFixedString($entryOffset + 8, 8),
            $reader->readU32LE($entryOffset),
            $reader->readU32LE($entryOffset + 4)
        );
    }

    public function entryCount(WadEntry $entry, int $recordSize): int {
        if ($recordSize <= 0) {
            return 0;
        }

        return intdiv($entry->size, $recordSize);
    }
}
