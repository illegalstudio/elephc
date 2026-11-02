<?php
// Raw C memory via FFI.
// This example uses libc allocation plus byte, 16-bit word, and raw string helpers.

extern "System" {
    function malloc(u64 $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, i32 $byte, u64 $count): ptr;
    function memcpy(ptr $dest, ptr $src, u64 $count): ptr;
}

$src = malloc(64);
$dst = malloc(64);

if (ptr_is_null($src) || ptr_is_null($dst)) {
    echo "allocation failed\n";
    exit(1);
}

memset($src, 0, 64);
ptr_write32($src, 305419896); // 0x12345678
ptr_write8(ptr_offset($src, 4), 90); // ASCII Z
ptr_write16(ptr_offset($src, 6), 8080);
$request_len = ptr_write_string(ptr_offset($src, 8), "GET / HTTP/1.1\r\n");

memcpy($dst, $src, 64);

echo "word = " . ptr_read32($dst) . "\n";
echo "byte = " . ptr_read8(ptr_offset($dst, 4)) . "\n";
echo "port = " . ptr_read16(ptr_offset($dst, 6)) . "\n";
echo "request = " . ptr_read_string(ptr_offset($dst, 8), $request_len);

free($dst);
free($src);

// 32-bit floats: C `float` struct fields and vertex data are 4 bytes each.
$floats = malloc(8);
ptr_write_f32($floats, 1.5);
ptr_write_f32(ptr_offset($floats, 4), 0.1);
echo "f32[0] = " . ptr_read_f32($floats) . "\n";
echo "f32[1] = " . ptr_read_f32(ptr_offset($floats, 4)) . "\n";   // 0.1 rounded to single precision
free($floats);
