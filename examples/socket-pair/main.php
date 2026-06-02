<?php
// stream_socket_pair() creates two already-connected sockets — useful for
// passing a channel between two parts of a program.

$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
echo "pair holds " . count($pair) . " sockets\n";

// Whatever is written on one end is readable on the other.
fwrite($pair[0], "message for the other end");
echo "received: " . fread($pair[1], 64) . "\n";

fwrite($pair[1], "and a reply");
echo "reply: " . fread($pair[0], 64) . "\n";

fclose($pair[0]);
fclose($pair[1]);
echo "socket pair closed\n";
