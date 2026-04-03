<?php

namespace Showcases\Doom\IO;

class BinaryReader {
    public $bytes;
    public $length;

    public function __construct(string $bytes) {
        $this->bytes = $bytes;
        $this->length = strlen($bytes);
    }

    public function length(): int {
        return $this->length;
    }

    public function readU8(int $offset): int {
        if ($offset < 0 || $offset >= $this->length) {
            return 0;
        }

        return ord(substr($this->bytes, $offset, 1));
    }

    public function readU16LE(int $offset): int {
        $b0 = $this->readU8($offset);
        $b1 = $this->readU8($offset + 1);
        return $b0 | ($b1 << 8);
    }

    public function readI16LE(int $offset): int {
        $value = $this->readU16LE($offset);
        if ($value >= 32768) {
            return $value - 65536;
        }
        return $value;
    }

    public function readU32LE(int $offset): int {
        $b0 = $this->readU8($offset);
        $b1 = $this->readU8($offset + 1);
        $b2 = $this->readU8($offset + 2);
        $b3 = $this->readU8($offset + 3);
        return $b0 | ($b1 << 8) | ($b2 << 16) | ($b3 << 24);
    }

    public function readFixedString(int $offset, int $width): string {
        $raw = substr($this->bytes, $offset, $width);
        $result = "";
        $i = 0;

        while ($i < strlen($raw)) {
            $ch = substr($raw, $i, 1);
            if (ord($ch) == 0) {
                break;
            }
            $result .= $ch;
            $i += 1;
        }

        return $result;
    }

    public function slice(int $offset, int $width): string {
        return substr($this->bytes, $offset, $width);
    }
}
