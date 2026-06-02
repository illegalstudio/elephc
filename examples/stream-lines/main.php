<?php
// stream_get_line() reads a stream up to a delimiter, stripping the delimiter.

file_put_contents("events.log", "boot\nready\nshutdown\n");

$f = fopen("events.log", "r");
while (!feof($f)) {
    $line = stream_get_line($f, 256, "\n");
    if ($line !== "") {
        echo "event: " . $line . "\n";
    }
}
fclose($f);

unlink("events.log");
