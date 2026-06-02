<?php
// stream_set_blocking() switches a socket between blocking and non-blocking I/O.

$server = stream_socket_server("tcp://127.0.0.1:8123");

// Put the listening socket into non-blocking mode.
if (stream_set_blocking($server, false)) {
    echo "listening socket is now non-blocking\n";
}

// Switch it back to blocking mode.
if (stream_set_blocking($server, true)) {
    echo "listening socket is back to blocking\n";
}

fclose($server);
