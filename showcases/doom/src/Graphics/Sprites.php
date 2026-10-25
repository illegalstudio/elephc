<?php

namespace Showcases\Doom\Graphics;

use Showcases\Doom\Wad\Wad;

// Sprite frames from the lumps between S_START and S_END. A lump name is a 4-letter
// sprite, a frame letter and a rotation digit (0 = seen the same from every side,
// 1-8 = front, then 45-degree steps), optionally followed by a second frame and
// rotation that reuse the picture mirrored. Each (sprite, frame) pair becomes one
// frame definition with eight rotation lumps.
final class Sprites {
    public int $count;
    public buffer<int> $spriteKey;
    public buffer<int> $frame;
    public buffer<int> $rotated;
    public buffer<int> $lump;
    public buffer<int> $flip;

    public function __construct() {
        $this->count = 0;
        $this->spriteKey = buffer_new<int>(1);
        $this->frame = buffer_new<int>(1);
        $this->rotated = buffer_new<int>(1);
        $this->lump = buffer_new<int>(8);
        $this->flip = buffer_new<int>(8);
    }

    public function load(Wad $wad): bool {
        int $start = $wad->find("S_START");
        int $end = $wad->find("S_END");
        if ($start < 0 || $end <= $start) {
            return false;
        }
        int $capacity = $end - $start;
        int $rotationSlots = $capacity * 8;
        buffer<int> $oldSpriteKey = $this->spriteKey;
        buffer_free($oldSpriteKey);
        buffer<int> $oldFrame = $this->frame;
        buffer_free($oldFrame);
        buffer<int> $oldRotated = $this->rotated;
        buffer_free($oldRotated);
        buffer<int> $oldLump = $this->lump;
        buffer_free($oldLump);
        buffer<int> $oldFlip = $this->flip;
        buffer_free($oldFlip);
        $this->spriteKey = buffer_new<int>($capacity);
        $this->frame = buffer_new<int>($capacity);
        $this->rotated = buffer_new<int>($capacity);
        $this->lump = buffer_new<int>($rotationSlots);
        $this->flip = buffer_new<int>($rotationSlots);
        for ($slot = 0; $slot < $rotationSlots; $slot++) {
            $this->lump[$slot] = -1;
        }

        int $firstLump = $start + 1;
        for ($lump = $firstLump; $lump < $end; $lump++) {
            int $key = $wad->lumpKey[$lump];
            int $sprite = $key & 4294967295;
            int $frameShift = $key >> 32;
            int $frameLetter = $frameShift & 255;
            int $rotationShift = $key >> 40;
            int $rotationDigit = $rotationShift & 255;
            if ($frameLetter < 65 || $rotationDigit < 48 || $rotationDigit > 56) {
                continue;
            }
            int $frameIndex = $frameLetter - 65;
            int $rotation = $rotationDigit - 48;
            $this->assign($sprite, $frameIndex, $rotation, $lump, 0);

            int $mirrorFrameShift = $key >> 48;
            int $mirrorFrame = $mirrorFrameShift & 255;
            int $mirrorRotationShift = $key >> 56;
            int $mirrorRotationDigit = $mirrorRotationShift & 255;
            if ($mirrorFrame >= 65 && $mirrorRotationDigit >= 48 && $mirrorRotationDigit <= 56) {
                int $mirrorIndex = $mirrorFrame - 65;
                int $mirrorRotation = $mirrorRotationDigit - 48;
                $this->assign($sprite, $mirrorIndex, $mirrorRotation, $lump, 1);
            }
        }
        return $this->count > 0;
    }

    public function assign(int $sprite, int $frameIndex, int $rotation, int $lump, int $flip): void {
        int $definition = $this->find($sprite, $frameIndex);
        if ($definition < 0) {
            $definition = $this->count;
            $this->spriteKey[$definition] = $sprite;
            $this->frame[$definition] = $frameIndex;
            $this->rotated[$definition] = 0;
            $this->count++;
        }
        int $base = $definition * 8;
        if ($rotation === 0) {
            for ($r = 0; $r < 8; $r++) {
                int $slot = $base + $r;
                $this->lump[$slot] = $lump;
                $this->flip[$slot] = $flip;
            }
            return;
        }
        int $rotationIndex = $rotation - 1;
        int $slot = $base + $rotationIndex;
        $this->lump[$slot] = $lump;
        $this->flip[$slot] = $flip;
        $this->rotated[$definition] = 1;
    }

    public function find(int $sprite, int $frameIndex): int {
        for ($d = 0; $d < $this->count; $d++) {
            if ($this->spriteKey[$d] === $sprite && $this->frame[$d] === $frameIndex) {
                return $d;
            }
        }
        return -1;
    }
}
