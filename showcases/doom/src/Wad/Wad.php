<?php

namespace Showcases\Doom\Wad;

// The whole WAD file in one byte buffer plus its lump directory. A lump name is
// compared as a 64-bit key (its eight upper-case bytes, little-endian), so lookups
// never build strings.
final class Wad {
    public buffer<u8> $data;
    public int $size;
    public string $kind;
    public int $lumpCount;
    public buffer<int> $lumpOffset;
    public buffer<int> $lumpSize;
    public buffer<int> $lumpKey;

    public function __construct() {
        $this->data = buffer_new<u8>(1);
        $this->size = 0;
        $this->kind = "";
        $this->lumpCount = 0;
        $this->lumpOffset = buffer_new<int>(1);
        $this->lumpSize = buffer_new<int>(1);
        $this->lumpKey = buffer_new<int>(1);
    }

    public function load(string $path): bool {
        if (!is_file($path)) {
            return false;
        }
        $read = file_get_contents($path);
        if (!is_string($read)) {
            return false;
        }
        string $bytes = (string)$read;
        int $size = strlen($bytes);
        if ($size < 12) {
            return false;
        }
        buffer<u8> $placeholder = $this->data;
        buffer_free($placeholder);
        $this->data = buffer_new<u8>($size);
        ptr_write_string(buffer_ptr($this->data), $bytes);
        $this->size = $size;
        int $magic = $this->keyAt(0);
        int $kindKey = $magic & 4294967295;
        $this->kind = $this->nameFromKey($kindKey);
        if ($this->kind !== "IWAD" && $this->kind !== "PWAD") {
            return false;
        }

        int $count = $this->i32(4);
        int $directory = $this->i32(8);
        int $directoryBytes = $count * 16;
        int $directoryEnd = $directory + $directoryBytes;
        if ($count <= 0 || $directory < 12 || $directoryEnd > $size) {
            return false;
        }
        $this->lumpCount = $count;
        buffer<int> $oldOffset = $this->lumpOffset;
        buffer_free($oldOffset);
        buffer<int> $oldSize = $this->lumpSize;
        buffer_free($oldSize);
        buffer<int> $oldKey = $this->lumpKey;
        buffer_free($oldKey);
        $this->lumpOffset = buffer_new<int>($count);
        $this->lumpSize = buffer_new<int>($count);
        $this->lumpKey = buffer_new<int>($count);
        for ($i = 0; $i < $count; $i++) {
            int $entryStart = $i * 16;
            int $entry = $directory + $entryStart;
            int $sizeAt = $entry + 4;
            int $nameAt = $entry + 8;
            $this->lumpOffset[$i] = $this->i32($entry);
            $this->lumpSize[$i] = $this->i32($sizeAt);
            $this->lumpKey[$i] = $this->keyAt($nameAt);
        }
        return true;
    }

    public function u8(int $offset): int {
        return $this->data[$offset];
    }

    public function u16(int $offset): int {
        int $next = $offset + 1;
        int $low = $this->data[$offset];
        int $high = $this->data[$next];
        int $shifted = $high << 8;
        int $value = $low | $shifted;
        return $value;
    }

    public function i16(int $offset): int {
        int $value = $this->u16($offset);
        if ($value >= 32768) {
            int $negative = $value - 65536;
            return $negative;
        }
        return $value;
    }

    public function i32(int $offset): int {
        int $highAt = $offset + 2;
        int $low = $this->u16($offset);
        int $high = $this->u16($highAt);
        int $shifted = $high << 16;
        int $value = $low | $shifted;
        if ($value >= 2147483648) {
            int $negative = $value - 4294967296;
            return $negative;
        }
        return $value;
    }

    // Eight name bytes up to the first NUL, upper-cased, packed little-endian.
    public function keyAt(int $offset): int {
        int $key = 0;
        for ($k = 0; $k < 8; $k++) {
            int $at = $offset + $k;
            int $byte = $this->data[$at];
            if ($byte === 0) {
                break;
            }
            if ($byte >= 97 && $byte <= 122) {
                int $upper = $byte - 32;
                $byte = $upper;
            }
            int $shift = $k * 8;
            int $placed = $byte << $shift;
            int $merged = $key | $placed;
            $key = $merged;
        }
        return $key;
    }

    public function key(string $name): int {
        string $upper = strtoupper($name);
        int $length = strlen($upper);
        if ($length > 8) {
            $length = 8;
        }
        int $key = 0;
        for ($k = 0; $k < $length; $k++) {
            int $byte = ord($upper[$k]);
            int $shift = $k * 8;
            int $placed = $byte << $shift;
            int $merged = $key | $placed;
            $key = $merged;
        }
        return $key;
    }

    public function nameFromKey(int $key): string {
        string $name = "";
        for ($k = 0; $k < 8; $k++) {
            int $shift = $k * 8;
            int $shifted = $key >> $shift;
            int $byte = $shifted & 255;
            if ($byte === 0) {
                break;
            }
            $name .= chr($byte);
        }
        return $name;
    }

    public function nameOf(int $lump): string {
        return $this->nameFromKey($this->lumpKey[$lump]);
    }

    // Last lump with this name, as DOOM resolves duplicates; -1 when missing.
    public function find(string $name): int {
        return $this->findKey($this->key($name));
    }

    public function findKey(int $key): int {
        int $last = $this->lumpCount - 1;
        for ($k = 0; $k < $this->lumpCount; $k++) {
            int $index = $last - $k;
            if ($this->lumpKey[$index] === $key) {
                return $index;
            }
        }
        return -1;
    }

    // Lump in the index range [$first, $end), searching backwards.
    public function findKeyBetween(int $key, int $first, int $end): int {
        int $count = $end - $first;
        int $last = $end - 1;
        for ($k = 0; $k < $count; $k++) {
            int $index = $last - $k;
            if ($this->lumpKey[$index] === $key) {
                return $index;
            }
        }
        return -1;
    }
}
