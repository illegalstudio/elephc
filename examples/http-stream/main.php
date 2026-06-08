<?php
// The http:// wrapper fetches a URL over HTTP as a readable stream.
// fopen() connects, sends an HTTP/1.0 GET, and exposes the response body
// with the headers stripped. Running this example requires network access.

$handle = fopen("http://example.com/", "r");
if ($handle === false) {
    echo "could not open the http:// stream (network access required)\n";
} else {
    $body = stream_get_contents($handle);
    fclose($handle);
    echo "fetched " . strlen($body) . " bytes over http://\n";
}

// file_get_contents() reads the whole body of an http/https/ftp/ftps URL in a
// single call, using the same wrappers as fopen() (false on a failed open).
$body2 = file_get_contents("http://example.com/");
if ($body2 === false) {
    echo "file_get_contents over http:// needs network access\n";
} else {
    echo "file_get_contents read " . strlen($body2) . " bytes over http://\n";
}
