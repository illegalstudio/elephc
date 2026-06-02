<?php
// A Unix-domain datagram exchange: a server socket bound to a filesystem
// path and a client socket connected to it. Datagrams flow from client to
// server with no listen/accept handshake, unlike the stream-oriented
// unix:// transport.

$path = "/tmp/elephc-udg-example.sock";
unlink($path);

$server = stream_socket_server("udg://" . $path);
$client = stream_socket_client("udg://" . $path);
echo "udg sockets opened\n";

// The client sends a datagram. fread on the server receives one whole
// datagram up to the requested length.
fwrite($client, "udg datagram payload");
echo "server received: " . fread($server, 64) . "\n";

fclose($client);
fclose($server);
unlink($path);
echo "udg sockets closed\n";
