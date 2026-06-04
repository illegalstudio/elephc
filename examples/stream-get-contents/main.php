<?php
// stream_get_contents() reads everything remaining from an open stream.

file_put_contents("poem.txt", "roses are red\nstreams are buffered\n");

// Read a whole file through its stream handle.
$handle = fopen("poem.txt", "r");
$whole = stream_get_contents($handle);
echo "whole file (" . strlen($whole) . " bytes):\n";
echo $whole;
fclose($handle);

// stream_get_contents() resumes from the current position, so a partial
// read followed by stream_get_contents() returns only the remainder.
$handle = fopen("poem.txt", "r");
$head = fread($handle, 13);
$tail = stream_get_contents($handle);
echo "head: " . $head . "\n";
echo "tail: " . $tail;
fclose($handle);

// The optional $length caps how many bytes are read, and $offset (>= 0) seeks
// the stream first — so you can pull a slice without a manual fseek().
$handle = fopen("poem.txt", "r");
echo "first 5 bytes: " . stream_get_contents($handle, 5) . "\n";
echo "7 bytes from offset 14: " . stream_get_contents($handle, 7, 14) . "\n";
fclose($handle);

unlink("poem.txt");
