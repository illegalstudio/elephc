<?php

namespace Showcases\Doom\Wad;

use Showcases\Doom\IO\BinaryReader;

class WadLoader {
    public function report(string $path): string {
        $bytes = file_get_contents($path);
        BinaryReader $reader = new BinaryReader($bytes);

        if ($reader->length() < 12) {
            return "";
        }

        $kind = $reader->readFixedString(0, 4);
        if ($kind !== "IWAD" && $kind !== "PWAD") {
            return "";
        }

        $entryCount = $reader->readU32LE(4);
        $directoryOffset = $reader->readU32LE(8);
        $report = "Loaded WAD: "
            . $kind
            . " | lumps: "
            . $entryCount
            . " | directory: "
            . $directoryOffset
            . "\n";

        if ($entryCount > 0) {
            $firstEntryOffsetInDir = $directoryOffset;
            $firstEntryOffset = $reader->readU32LE($firstEntryOffsetInDir);
            $firstEntrySize = $reader->readU32LE($firstEntryOffsetInDir + 4);
            $firstEntryName = $reader->readFixedString($firstEntryOffsetInDir + 8, 8);
            $report .= "First lump: "
                . $firstEntryName
                . " @ "
                . $firstEntryOffset
                . " ("
                . $firstEntrySize
                . " bytes)\n";
        }

        return $report;
    }
}
