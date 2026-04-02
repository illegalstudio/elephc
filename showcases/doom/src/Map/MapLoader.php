<?php

namespace Showcases\Doom\Map;

use Showcases\Doom\IO\BinaryReader;
use Showcases\Doom\Wad\WadFile;

class MapLoader {
    public function load(WadFile $wad, string $mapName): MapData {
        if (!$wad->isValid()) {
            return $this->invalid($mapName);
        }

        $bytes = file_get_contents($wad->path);
        BinaryReader $reader = new BinaryReader($bytes);
        $markerIndex = $this->findMarkerIndex($reader, $wad, $mapName);
        if ($markerIndex < 0) {
            return $this->invalid($mapName);
        }

        $thingsSize = $this->findMapEntrySize($reader, $wad, $markerIndex, "THINGS");
        $linedefsSize = $this->findMapEntrySize($reader, $wad, $markerIndex, "LINEDEFS");
        $sidedefsSize = $this->findMapEntrySize($reader, $wad, $markerIndex, "SIDEDEFS");
        $vertexesSize = $this->findMapEntrySize($reader, $wad, $markerIndex, "VERTEXES");
        $segsSize = $this->findMapEntrySize($reader, $wad, $markerIndex, "SEGS");
        $ssectorsSize = $this->findMapEntrySize($reader, $wad, $markerIndex, "SSECTORS");
        $nodesSize = $this->findMapEntrySize($reader, $wad, $markerIndex, "NODES");
        $sectorsSize = $this->findMapEntrySize($reader, $wad, $markerIndex, "SECTORS");

        if (
            $thingsSize < 0
            || $linedefsSize < 0
            || $sidedefsSize < 0
            || $vertexesSize < 0
            || $segsSize < 0
            || $ssectorsSize < 0
            || $nodesSize < 0
            || $sectorsSize < 0
        ) {
            return $this->invalid($mapName);
        }

        return new MapData(
            $mapName,
            $markerIndex,
            $this->entryCount($thingsSize, 10),
            $this->entryCount($linedefsSize, 14),
            $this->entryCount($sidedefsSize, 30),
            $this->entryCount($vertexesSize, 4),
            $this->entryCount($segsSize, 12),
            $this->entryCount($ssectorsSize, 4),
            $this->entryCount($nodesSize, 28),
            $this->entryCount($sectorsSize, 26)
        );
    }

    public function invalid(string $mapName): MapData {
        return new MapData($mapName, -1, 0, 0, 0, 0, 0, 0, 0, 0);
    }

    public function findMapEntrySize(BinaryReader $reader, WadFile $wad, int $markerIndex, string $entryName): int {
        $i = $markerIndex + 1;
        $limit = $markerIndex + 11;

        while ($i < $wad->countEntries() && $i <= $limit) {
            if ($this->entryNameAt($reader, $wad, $i) === $entryName) {
                return $this->entrySizeAt($reader, $wad, $i);
            }
            $i += 1;
        }

        return -1;
    }

    public function entryCount(int $entrySize, int $recordSize): int {
        if ($recordSize <= 0) {
            return 0;
        }

        return intdiv($entrySize, $recordSize);
    }

    public function entryNameAt(BinaryReader $reader, WadFile $wad, int $index): string {
        if ($index < 0 || $index >= $wad->countEntries()) {
            return "";
        }

        return $reader->readFixedString($wad->directoryOffset + ($index * 16) + 8, 8);
    }

    public function entrySizeAt(BinaryReader $reader, WadFile $wad, int $index): int {
        if ($index < 0 || $index >= $wad->countEntries()) {
            return 0;
        }

        return $reader->readU32LE($wad->directoryOffset + ($index * 16) + 4);
    }

    public function findMarkerIndex(BinaryReader $reader, WadFile $wad, string $mapName): int {
        $i = 0;
        while ($i < $wad->countEntries()) {
            if ($this->entryNameAt($reader, $wad, $i) === $mapName) {
                return $i;
            }
            $i += 1;
        }

        return -1;
    }
}
