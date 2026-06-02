<?php
// stream_get_meta_data() reports live information about an open stream:
// its mode, type, whether it is seekable, and whether end-of-file has
// been reached.

$file = fopen("notes.txt", "w");
fwrite($file, "first line\nsecond line\n");
fclose($file);

$file = fopen("notes.txt", "r");
$meta = stream_get_meta_data($file);

echo "mode:        " . $meta["mode"] . "\n";
echo "stream_type: " . $meta["stream_type"] . "\n";
echo "seekable:    " . ($meta["seekable"] ? "yes" : "no") . "\n";
echo "blocked:     " . ($meta["blocked"] ? "yes" : "no") . "\n";
echo "eof:         " . ($meta["eof"] ? "yes" : "no") . "\n";

// Read the whole file, then ask again — the eof flag has flipped.
fread($file, 1024);
fread($file, 1024);
$meta = stream_get_meta_data($file);
echo "eof after reading everything: " . ($meta["eof"] ? "yes" : "no") . "\n";

fclose($file);
unlink("notes.txt");
