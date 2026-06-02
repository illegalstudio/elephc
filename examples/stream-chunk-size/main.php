<?php

// stream_set_chunk_size() returns the PREVIOUS chunk size, so it supports the
// usual save / change / restore pattern. The first call on a stream reports the
// 8192 default.
//
// stream_set_read_buffer() / stream_set_write_buffer() return 0 ("success"):
// elephc streams are unbuffered (direct read/write syscalls), so the buffer size
// has no effect — 0 is the correct result for an unbuffered stream.

$stream = fopen('php://memory', 'r+');

$previous = stream_set_chunk_size($stream, 4096);   // default → 8192
echo "previous chunk size: " . $previous . "\n";

$previous = stream_set_chunk_size($stream, 16384);  // → 4096
echo "previous chunk size: " . $previous . "\n";

// Restore the original chunk size; the return is what we just set (16384).
stream_set_chunk_size($stream, $previous);

echo "read buffer result: " . stream_set_read_buffer($stream, 0) . "\n";
echo "write buffer result: " . stream_set_write_buffer($stream, 0) . "\n";

fclose($stream);
