<?php

namespace Showcases\Doom\Wad;

class WadFile {
    public $kind;
    public $entryCount;
    public $directoryOffset;
    public $hasFirstEntry;
    public $firstEntryName;
    public $firstEntryOffset;
    public $firstEntrySize;

    public function __construct() {
        $this->kind = "";
        $this->entryCount = 0;
        $this->directoryOffset = 0;
        $this->hasFirstEntry = 0;
        $this->firstEntryName = "";
        $this->firstEntryOffset = 0;
        $this->firstEntrySize = 0;
    }

    public function summary(): string {
        return $this->kind
            . " | lumps: "
            . $this->entryCount
            . " | directory: "
            . $this->directoryOffset;
    }
}
