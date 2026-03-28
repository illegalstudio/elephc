<?php
// Raw C memory via FFI.
// This example uses libc allocation and byte/word pointer helpers.

extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, int $byte, int $count): ptr;
    function memcpy(ptr $dest, ptr $src, int $count): ptr;
}

$src = malloc(8);
$dst = malloc(8);

if (ptr_is_null($src) || ptr_is_null($dst)) {
    echo "allocation failed\n";
    exit(1);
}

memset($src, 0, 8);
ptr_write32($src, 305419896); // 0x12345678
ptr_write8(ptr_offset($src, 4), 90); // ASCII Z

memcpy($dst, $src, 8);

echo "word = " . ptr_read32($dst) . "\n";
echo "byte = " . ptr_read8(ptr_offset($dst, 4)) . "\n";

free($dst);
free($src);
