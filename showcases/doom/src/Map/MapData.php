<?php

namespace Showcases\Doom\Map;

class MapData {
    public $mapName;
    public $markerIndex;
    public $thingCount;
    public $linedefCount;
    public $sidedefCount;
    public $vertexCount;
    public $segCount;
    public $subSectorCount;
    public $nodeCount;
    public $sectorCount;

    public function __construct(
        string $mapName,
        int $markerIndex,
        int $thingCount,
        int $linedefCount,
        int $sidedefCount,
        int $vertexCount,
        int $segCount,
        int $subSectorCount,
        int $nodeCount,
        int $sectorCount
    ) {
        $this->mapName = $mapName;
        $this->markerIndex = $markerIndex;
        $this->thingCount = $thingCount;
        $this->linedefCount = $linedefCount;
        $this->sidedefCount = $sidedefCount;
        $this->vertexCount = $vertexCount;
        $this->segCount = $segCount;
        $this->subSectorCount = $subSectorCount;
        $this->nodeCount = $nodeCount;
        $this->sectorCount = $sectorCount;
    }

    public function isValid(): bool {
        return $this->markerIndex >= 0;
    }

    public function summary(): string {
        return $this->mapName
            . ": "
            . $this->vertexCount
            . " vertices, "
            . $this->linedefCount
            . " linedefs, "
            . $this->sidedefCount
            . " sidedefs, "
            . $this->sectorCount
            . " sectors, "
            . $this->thingCount
            . " things, "
            . $this->segCount
            . " segs, "
            . $this->subSectorCount
            . " subsectors, "
            . $this->nodeCount
            . " nodes";
    }
}
