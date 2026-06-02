<?php
// A complete TCP exchange: server, client, accept, data transfer, shutdown.

$server = stream_socket_server("tcp://127.0.0.1:8733");
// The client connects by host name; "localhost" is resolved to 127.0.0.1
// through the system resolver before the socket connects.
$client = stream_socket_client("tcp://localhost:8733");

// The server accepts the pending connection from the client.
$connection = stream_socket_accept($server);
echo "connection accepted\n";

// stream_socket_get_name() reports the server's bound address.
echo "server bound at: " . stream_socket_get_name($server, false) . "\n";

// Give the connection a receive timeout so a stalled peer cannot block reads.
stream_set_timeout($connection, 5);

// The client sends a message; the server reads it off the connection.
fwrite($client, "hello from the client");
$message = fread($connection, 64);
echo "server received: " . $message . "\n";

// stream_socket_sendto() and stream_socket_recvfrom() exchange one more
// message over the same connected socket pair.
stream_socket_sendto($client, "one more message");
$extra = stream_socket_recvfrom($connection, 64);
echo "server also received: " . $extra . "\n";

// Shut the connection down before closing the descriptors.
stream_socket_shutdown($connection, 2);
echo "connection shut down\n";

fclose($connection);
fclose($client);
fclose($server);
echo "all sockets closed\n";
