<?php
// stream_select() blocks until one or more streams are ready for I/O, or
// until the timeout elapses. Each array is rewritten to its ready subset.

$pair = stream_socket_pair(1, 1, 0);
$writer = $pair[0];
$reader = $pair[1];

fwrite($writer, "hello");

$read = [$reader];
$write = [];
$except = [];
$ready = stream_select($read, $write, $except, 1, 0);

echo "streams ready: " . $ready . "\n";
echo "data waiting:  " . fread($reader, 16) . "\n";
