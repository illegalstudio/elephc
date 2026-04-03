<?php

namespace Showcases\Doom\Wad;

class WadFile {
    public $path;
    public $kind;
    public $entryCount;
    public $directoryOffset;
    public $firstEntryName;
    public $firstEntryOffset;
    public $firstEntrySize;

    public function __construct(
        string $path,
        string $kind,
        int $entryCount,
        int $directoryOffset
    ) {
        $this->path = $path;
        $this->kind = $kind;
        $this->entryCount = $entryCount;
        $this->directoryOffset = $directoryOffset;
        $this->firstEntryName = "";
        $this->firstEntryOffset = 0;
        $this->firstEntrySize = 0;
    }

    public function isValid(): bool {
        return $this->kind !== "";
    }

    public function summary(): string {
        return $this->kind
            . " | lumps: "
            . $this->entryCount
            . " | directory: "
            . $this->directoryOffset;
    }

    public function countEntries(): int {
        return $this->entryCount;
    }

    public function firstEntrySummary(): string {
        if ($this->firstEntryName === "") {
            return "";
        }

        return $this->firstEntryName
            . " @ "
            . $this->firstEntryOffset
            . " ("
            . $this->firstEntrySize
            . " bytes)";
    }
}
