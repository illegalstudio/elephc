<?php
// stream_set_blocking() switches a socket between blocking and non-blocking I/O.

$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);

// Put one end into non-blocking mode. A read with no pending bytes returns
// immediately and does not mark the stream EOF.
if (stream_set_blocking($pair[0], false)) {
    echo "socket is now non-blocking\n";
}

if (fread($pair[0], 5) === "") {
    echo feof($pair[0]) ? "unexpected eof\n" : "no bytes yet\n";
}

fwrite($pair[1], "ready\n");
echo fgets($pair[0]);

// Switch it back to blocking mode before closing.
if (stream_set_blocking($pair[0], true)) {
    echo "socket is back to blocking\n";
}

fclose($pair[0]);
fclose($pair[1]);
