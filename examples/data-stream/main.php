<?php
// data:// URIs (RFC 2397) carry their content inline. fopen() decodes the
// URI and hands back an ordinary readable stream — handy for embedding a
// small fixed payload without shipping a separate file.

// A ;base64 payload is base64-decoded.
$encoded = fopen("data://text/plain;base64,SGVsbG8sIHdvcmxkIQ==", "r");
echo "base64 payload: " . fread($encoded, 64) . "\n";
fclose($encoded);

// A plain payload is percent-decoded: %HH escapes, and + becomes a space.
$plain = fopen("data://text/plain,inline%20text%2C+decoded", "r");
echo "plain payload:  " . fread($plain, 64) . "\n";
fclose($plain);

// The result is a normal seekable stream.
$seekable = fopen("data://,0123456789", "r");
fseek($seekable, 5);
echo "from offset 5:  " . fread($seekable, 64) . "\n";
fclose($seekable);
