<?php

namespace Showcases\Doom\Wad;

class WadFile {
    public string $path;
    public string $kind;
    public int $entryCount;
    public int $directoryOffset;
    public string $firstEntryName;
    public int $firstEntryOffset;
    public int $firstEntrySize;

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
