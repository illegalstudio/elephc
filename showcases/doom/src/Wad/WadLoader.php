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
        if ($directoryOffset <= 0 || $directoryOffset + ($entryCount * 16) > $reader->length()) {
            return $this->invalid($path);
        }

        $wad = new WadFile($path, $kind, $entryCount, $directoryOffset);
        int $i = 0;

        while ($i < $entryCount && $i < 1) {
            int $entryOffset = $directoryOffset + ($i * 16);
            $wad->firstEntryName = $reader->readFixedString($entryOffset + 8, 8);
            $wad->firstEntryOffset = $reader->readU32LE($entryOffset);
            $wad->firstEntrySize = $reader->readU32LE($entryOffset + 4);
            $i += 1;
        }

        return $wad;
    }

    public function invalid(string $path): WadFile {
        return new WadFile($path, "", 0, 0);
    }
}
