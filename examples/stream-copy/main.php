<?php
// stream_copy_to_stream() pipes every remaining byte from one stream to another.

file_put_contents("source.txt", "elephc copies streams byte by byte\n");

$from = fopen("source.txt", "r");
$to = fopen("backup.txt", "w");
$copied = stream_copy_to_stream($from, $to);
fclose($from);
fclose($to);

echo "copied " . $copied . " bytes\n";
echo "backup.txt now holds:\n";
echo file_get_contents("backup.txt");

// The optional $length caps the copy and $offset (>= 0) seeks the source first.
$from = fopen("source.txt", "r");
$to = fopen("partial.txt", "w");
$n = stream_copy_to_stream($from, $to, 6);
fclose($from);
fclose($to);
echo "partial copy (" . $n . " bytes): " . file_get_contents("partial.txt") . "\n";

unlink("source.txt");
unlink("backup.txt");
unlink("partial.txt");
