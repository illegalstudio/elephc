<?php
// php://memory is an in-memory stream. It behaves like any file handle —
// fwrite, fread, fseek and rewind all work — but its contents live in an
// anonymous temp buffer instead of a named file on disk. php://temp behaves
// identically.

$stream = fopen("php://memory", "r+");

// Build up some content with successive writes.
fwrite($stream, "alpha ");
fwrite($stream, "beta ");
fwrite($stream, "gamma");

// Rewind to the start and read the whole buffer back.
rewind($stream);
echo "contents: " . fread($stream, 1024) . "\n";

// Seek to an arbitrary offset and read from there.
fseek($stream, 6);
echo "from offset 6: " . fread($stream, 1024) . "\n";

fclose($stream);
echo "memory stream closed\n";
