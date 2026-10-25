<?php

namespace Showcases\Doom\Graphics;

use Showcases\Doom\Wad\Wad;

// Wall textures from TEXTURE1/TEXTURE2, each composed once from its PNAMES patches
// into one column-major byte buffer: texel (u, v) of texture t is at
// $offset[t] + u * $height[t] + v. Holes stay palette index 0, and $opaque marks
// (with 1) the texels some patch covered, for see-through middle textures.
final class Textures {
    public int $count;
    public buffer<int> $width;
    public buffer<int> $height;
    public buffer<int> $offset;
    public buffer<int> $key;
    public buffer<int> $translation;
    public buffer<u8> $texels;
    public buffer<u8> $opaque;
    public Animations $animations;
    public int $sky;

    public function __construct() {
        $this->count = 0;
        $this->width = buffer_new<int>(1);
        $this->height = buffer_new<int>(1);
        $this->offset = buffer_new<int>(1);
        $this->key = buffer_new<int>(1);
        $this->translation = buffer_new<int>(1);
        $this->texels = buffer_new<u8>(1);
        $this->opaque = buffer_new<u8>(1);
        $this->animations = new Animations();
        $this->sky = -1;
    }

    public function load(Wad $wad): bool {
        int $pnames = $wad->find("PNAMES");
        if ($pnames < 0) {
            return false;
        }
        int $pnamesAt = $wad->lumpOffset[$pnames];
        int $patchCount = $wad->i32($pnamesAt);
        buffer<int> $patchLump = buffer_new<int>($patchCount + 1);
        int $namesAt = $pnamesAt + 4;
        for ($p = 0; $p < $patchCount; $p++) {
            int $nameOffset = $p * 8;
            int $nameAt = $namesAt + $nameOffset;
            $patchLump[$p] = $wad->findKey($wad->keyAt($nameAt));
        }

        int $first = $wad->find("TEXTURE1");
        int $second = $wad->find("TEXTURE2");
        int $firstCount = 0;
        int $secondCount = 0;
        if ($first >= 0) {
            $firstCount = $wad->i32($wad->lumpOffset[$first]);
        }
        if ($second >= 0) {
            $secondCount = $wad->i32($wad->lumpOffset[$second]);
        }
        int $count = $firstCount + $secondCount;
        if ($count <= 0) {
            buffer_free($patchLump);
            return false;
        }
        $this->count = $count;
        buffer<int> $oldWidth = $this->width;
        buffer_free($oldWidth);
        buffer<int> $oldHeight = $this->height;
        buffer_free($oldHeight);
        buffer<int> $oldOffset = $this->offset;
        buffer_free($oldOffset);
        buffer<int> $oldKey = $this->key;
        buffer_free($oldKey);
        buffer<int> $oldTranslation = $this->translation;
        buffer_free($oldTranslation);
        $this->width = buffer_new<int>($count);
        $this->height = buffer_new<int>($count);
        $this->offset = buffer_new<int>($count);
        $this->key = buffer_new<int>($count);
        $this->translation = buffer_new<int>($count);
        buffer<int> $definition = buffer_new<int>($count);

        // headers: name, size and place in the texel buffer
        int $total = 0;
        for ($t = 0; $t < $count; $t++) {
            int $lump = $first;
            int $local = $t;
            if ($t >= $firstCount) {
                $lump = $second;
                int $secondIndex = $t - $firstCount;
                $local = $secondIndex;
            }
            int $lumpAt = $wad->lumpOffset[$lump];
            int $entry = $local * 4;
            int $pointerAt = $lumpAt + 4;
            int $pointer = $pointerAt + $entry;
            int $defAt = $lumpAt + $wad->i32($pointer);
            int $widthAt = $defAt + 12;
            int $heightAt = $defAt + 14;
            int $width = $wad->i16($widthAt);
            int $height = $wad->i16($heightAt);
            $definition[$t] = $defAt;
            $this->key[$t] = $wad->keyAt($defAt);
            $this->width[$t] = $width;
            $this->height[$t] = $height;
            $this->offset[$t] = $total;
            $this->translation[$t] = $t;
            int $area = $width * $height;
            int $grown = $total + $area;
            $total = $grown;
        }

        buffer<u8> $oldTexels = $this->texels;
        buffer_free($oldTexels);
        buffer<u8> $oldOpaque = $this->opaque;
        buffer_free($oldOpaque);
        $this->texels = buffer_new<u8>($total + 1);
        $this->opaque = buffer_new<u8>($total + 1);
        for ($t = 0; $t < $count; $t++) {
            int $defAt = $definition[$t];
            int $patchCountAt = $defAt + 20;
            int $patches = $wad->i16($patchCountAt);
            int $patchesAt = $defAt + 22;
            for ($p = 0; $p < $patches; $p++) {
                int $patchEntry = $p * 10;
                int $patchAt = $patchesAt + $patchEntry;
                int $originYAt = $patchAt + 2;
                int $indexAt = $patchAt + 4;
                int $patchIndex = $wad->i16($indexAt);
                if ($patchIndex < 0 || $patchIndex >= $patchCount) {
                    continue;
                }
                int $lump = $patchLump[$patchIndex];
                if ($lump < 0) {
                    continue;
                }
                $this->composite($wad, $t, $lump, $wad->i16($patchAt), $wad->i16($originYAt));
            }
        }
        buffer_free($definition);
        buffer_free($patchLump);

        $this->sky = $this->find($wad->key("SKY1"));
        $this->addAnimation($wad, "BLODGR1", "BLODGR4");
        $this->addAnimation($wad, "SLADRIP1", "SLADRIP3");
        $this->addAnimation($wad, "BLODRIP1", "BLODRIP4");
        $this->addAnimation($wad, "FIREWALA", "FIREWALL");
        $this->addAnimation($wad, "GSTFONT1", "GSTFONT3");
        $this->addAnimation($wad, "FIRELAV3", "FIRELAVA");
        $this->addAnimation($wad, "FIREMAG1", "FIREMAG3");
        $this->addAnimation($wad, "FIREBLU1", "FIREBLU2");
        $this->addAnimation($wad, "ROCKRED1", "ROCKRED3");
        $this->addAnimation($wad, "BFALL1", "BFALL4");
        $this->addAnimation($wad, "SFALL1", "SFALL4");
        $this->addAnimation($wad, "WFALL1", "WFALL4");
        $this->addAnimation($wad, "DBRAIN1", "DBRAIN4");
        return true;
    }

    // Copies one patch's posts into texture $t at its origin, clipped to the texture.
    public function composite(Wad $wad, int $t, int $lump, int $originX, int $originY): void {
        buffer<u8> $data = $wad->data;
        buffer<u8> $texels = $this->texels;
        buffer<u8> $opaque = $this->opaque;
        int $patchAt = $wad->lumpOffset[$lump];
        int $patchWidth = $wad->i16($patchAt);
        int $width = $this->width[$t];
        int $height = $this->height[$t];
        int $base = $this->offset[$t];
        int $columnsAt = $patchAt + 8;
        for ($c = 0; $c < $patchWidth; $c++) {
            int $tx = $originX + $c;
            if ($tx < 0 || $tx >= $width) {
                continue;
            }
            int $columnEntry = $c * 4;
            int $columnPointer = $columnsAt + $columnEntry;
            int $post = $patchAt + $wad->i32($columnPointer);
            int $columnStart = $tx * $height;
            int $columnBase = $base + $columnStart;
            while (true) {
                int $topDelta = $data[$post];
                if ($topDelta === 255) {
                    break;
                }
                int $lengthAt = $post + 1;
                int $length = $data[$lengthAt];
                int $pixelsAt = $post + 3;
                int $postTop = $originY + $topDelta;
                for ($k = 0; $k < $length; $k++) {
                    int $ty = $postTop + $k;
                    if ($ty >= 0 && $ty < $height) {
                        int $source = $pixelsAt + $k;
                        int $target = $columnBase + $ty;
                        $texels[$target] = $data[$source];
                        $opaque[$target] = 1;
                    }
                }
                int $postBytes = $length + 4;
                int $next = $post + $postBytes;
                $post = $next;
            }
        }
    }

    public function addAnimation(Wad $wad, string $firstName, string $lastName): void {
        $this->animations->add($this->find($wad->key($firstName)), $this->find($wad->key($lastName)));
    }

    public function find(int $key): int {
        for ($t = 0; $t < $this->count; $t++) {
            if ($this->key[$t] === $key) {
                return $t;
            }
        }
        return -1;
    }

    public function animate(int $tic): void {
        $this->animations->apply($this->translation, $tic);
    }
}
