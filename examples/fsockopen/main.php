<?php
// fsockopen() opens a raw TCP connection and returns it as a stream.
// The by-reference $errno / $errstr arguments report why a connection
// failed. Running this example requires outbound network access.

$errno = 0;
$errstr = "";
$sock = fsockopen("example.com", 80, $errno, $errstr);

if ($sock === false) {
    echo "connection failed: $errstr ($errno)\n";
} else {
    // Send a minimal HTTP request over the raw socket and read the status
    // line the server replies with.
    fwrite($sock, "GET / HTTP/1.0\r\nHost: example.com\r\nConnection: close\r\n\r\n");
    $status = fread($sock, 12);
    fclose($sock);
    echo "connected; server replied: " . $status . "\n";
}
