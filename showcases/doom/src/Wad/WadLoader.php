<?php

namespace Showcases\Doom\Wad;

use Showcases\Doom\IO\BinaryReader;

class WadLoader {
    public function load(string $path): WadFile {
        if (!file_exists($path)) {
            return $this->invalid($path);
        }

        $bytes = file_get_contents($path);
        BinaryReader $reader = new BinaryReader($bytes);

        if ($reader->length() < 12) {
            return $this->invalid($path);
        }

        $kind = $reader->readFixedString(0, 4);
        if ($kind !== "IWAD" && $kind !== "PWAD") {
            return $this->invalid($path);
        }

        $entryCount = $reader->readU32LE(4);
        $directoryOffset = $reader->readU32LE(8);
        $firstEntryName = "";
        $firstEntryOffset = 0;
        $firstEntrySize = 0;

        if ($entryCount > 0) {
            $firstEntryOffset = $reader->readU32LE($directoryOffset);
            $firstEntrySize = $reader->readU32LE($directoryOffset + 4);
            $firstEntryName = $reader->readFixedString($directoryOffset + 8, 8);
        }

        $wad = new WadFile($path, $kind, $entryCount, $directoryOffset);
        $wad->firstEntryName = $firstEntryName;
        $wad->firstEntryOffset = $firstEntryOffset;
        $wad->firstEntrySize = $firstEntrySize;
        return $wad;
    }

    public function invalid(string $path): WadFile {
        return new WadFile($path, "", 0, 0);
    }
}
