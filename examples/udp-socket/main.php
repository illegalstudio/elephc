<?php
// A UDP datagram exchange: a server socket bound to a port, a client socket
// connected to it, and one datagram sent from client to server. The server
// receives the datagram with stream_socket_recvfrom(), whose optional fourth
// argument reports the address of whoever sent it.

$server = stream_socket_server("udp://127.0.0.1:8755");
$client = stream_socket_client("udp://127.0.0.1:8755");
echo "udp sockets opened\n";

// The client sends a datagram; the server receives both the payload and the
// sender address. $sender must be a string variable — it is overwritten.
fwrite($client, "datagram payload");
$sender = "";
$message = stream_socket_recvfrom($server, 64, 0, $sender);
echo "server received: " . $message . "\n";
echo "sender address: " . $sender . "\n";

fclose($client);
fclose($server);
echo "udp sockets closed\n";
