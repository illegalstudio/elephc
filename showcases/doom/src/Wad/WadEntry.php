<?php

namespace Showcases\Doom\Wad;

class WadEntry {
    public $name;
    public $offset;
    public $size;

    public function __construct(string $name, int $offset, int $size) {
        $this->name = $name;
        $this->offset = $offset;
        $this->size = $size;
    }

    public function isNamed(string $name): bool {
        return $this->name === $name;
    }

    public function summary(): string {
        return $this->name . " @ " . $this->offset . " (" . $this->size . " bytes)";
    }
}
