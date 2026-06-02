<?php
// A Unix-domain socket exchange: a server bound to a filesystem path, a
// client connected to it, and a message passed over the connection.

$path = "/tmp/elephc-unix-example.sock";
unlink($path);

$server = stream_socket_server("unix://" . $path);
$client = stream_socket_client("unix://" . $path);
$connection = stream_socket_accept($server);
echo "unix connection accepted\n";

// The client sends a message; the server reads it off the connection.
fwrite($client, "hello over a unix socket");
$message = fread($connection, 64);
echo "server received: " . $message . "\n";

fclose($connection);
fclose($client);
fclose($server);
unlink($path);
echo "unix socket closed\n";
