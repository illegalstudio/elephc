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

final class MapData {
    public string $mapName;
    public int $markerIndex;
    public int $thingCount;
    public int $linedefCount;
    public int $sidedefCount;
    public int $vertexCount;
    public int $segCount;
    public int $subSectorCount;
    public int $nodeCount;
    public int $sectorCount;
    public int $minX;
    public int $maxX;
    public int $minY;
    public int $maxY;
    public int $playerStartX;
    public int $playerStartY;
    public int $playerStartAngle;
    public buffer<Thing> $things;
    public buffer<Linedef> $linedefs;
    public buffer<Sidedef> $sidedefs;
    public buffer<Vertex> $vertexes;
    public buffer<Seg> $segs;
    public buffer<SubSector> $subSectors;
    public buffer<Node> $nodes;
    public buffer<Sector> $sectors;
    // BLOCKMAP: 128-unit cells, each a list of linedef indices ending in -1
    public int $blockOriginX;
    public int $blockOriginY;
    public int $blockColumns;
    public int $blockRows;
    public buffer<int> $blockCell;
    public buffer<int> $blockList;

    public function __construct(string $mapName, int $markerIndex) {
        $this->mapName = $mapName;
        $this->markerIndex = $markerIndex;
        $this->thingCount = 0;
        $this->linedefCount = 0;
        $this->sidedefCount = 0;
        $this->vertexCount = 0;
        $this->segCount = 0;
        $this->subSectorCount = 0;
        $this->nodeCount = 0;
        $this->sectorCount = 0;
        $this->minX = 0;
        $this->maxX = 0;
        $this->minY = 0;
        $this->maxY = 0;
        $this->playerStartX = 0;
        $this->playerStartY = 0;
        $this->playerStartAngle = 0;
        $this->things = buffer_new<Thing>(1);
        $this->linedefs = buffer_new<Linedef>(1);
        $this->sidedefs = buffer_new<Sidedef>(1);
        $this->vertexes = buffer_new<Vertex>(1);
        $this->segs = buffer_new<Seg>(1);
        $this->subSectors = buffer_new<SubSector>(1);
        $this->nodes = buffer_new<Node>(1);
        $this->sectors = buffer_new<Sector>(1);
        $this->blockOriginX = 0;
        $this->blockOriginY = 0;
        $this->blockColumns = 0;
        $this->blockRows = 0;
        $this->blockCell = buffer_new<int>(1);
        $this->blockList = buffer_new<int>(1);
    }

    // Frees every record buffer. Each property always holds a live buffer of its own
    // (the loader frees a placeholder before it stores the real one), so this is the
    // map's only cleanup; the game calls it once nothing reads the map any more.
    public function release(): void {
        buffer<Thing> $things = $this->things;
        buffer_free($things);
        buffer<Linedef> $linedefs = $this->linedefs;
        buffer_free($linedefs);
        buffer<Sidedef> $sidedefs = $this->sidedefs;
        buffer_free($sidedefs);
        buffer<Vertex> $vertexes = $this->vertexes;
        buffer_free($vertexes);
        buffer<Seg> $segs = $this->segs;
        buffer_free($segs);
        buffer<SubSector> $subSectors = $this->subSectors;
        buffer_free($subSectors);
        buffer<Node> $nodes = $this->nodes;
        buffer_free($nodes);
        buffer<Sector> $sectors = $this->sectors;
        buffer_free($sectors);
        buffer<int> $blockCell = $this->blockCell;
        buffer_free($blockCell);
        buffer<int> $blockList = $this->blockList;
        buffer_free($blockList);
    }

    public function isValid(): bool {
        return $this->markerIndex >= 0;
    }

    public function summary(): string {
        return $this->mapName
            . ": " . $this->vertexCount . " vertices, "
            . $this->linedefCount . " linedefs, "
            . $this->sidedefCount . " sidedefs, "
            . $this->sectorCount . " sectors, "
            . $this->thingCount . " things, "
            . $this->segCount . " segs, "
            . $this->subSectorCount . " subsectors, "
            . $this->nodeCount . " nodes";
    }
}
