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
    public $minX;
    public $maxX;
    public $minY;
    public $maxY;
    public $playerStartX;
    public $playerStartY;
    public $playerStartAngle;
    public $things;
    public $linedefs;
    public $sidedefs;
    public $vertexes;
    public $segs;
    public $subSectors;
    public $nodes;
    public $sectors;
    public $paletteReds;
    public $paletteGreens;
    public $paletteBlues;
    public $paletteCount;

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
        $this->things = 0;
        $this->linedefs = 0;
        $this->sidedefs = 0;
        $this->vertexes = 0;
        $this->segs = 0;
        $this->subSectors = 0;
        $this->nodes = 0;
        $this->sectors = 0;
        $this->paletteReds = [];
        $this->paletteGreens = [];
        $this->paletteBlues = [];
        $this->paletteCount = 0;
    }

    public function isValid(): bool {
        return $this->markerIndex >= 0;
    }

    public function hasBounds(): bool {
        return $this->vertexCount > 0;
    }

    public function hasPlayerStart(): bool {
        return $this->thingCount > 0;
    }

    public function summary(): string {
        string $summary = $this->mapName
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

        if ($this->hasBounds()) {
            $summary .= " | bounds: (" . $this->minX . ", " . $this->minY . ") -> ("
                . $this->maxX . ", " . $this->maxY . ")";
        }

        if ($this->hasPlayerStart()) {
            $summary .= " | player: (" . $this->playerStartX . ", " . $this->playerStartY . ") @ "
                . $this->playerStartAngle;
        }

        return $summary;
    }
}
