<?php
// The https:// wrapper fetches a URL over TLS as a readable stream.
// elephc links the elephc-tls staticlib (rustls + ring + webpki-roots) into
// programs that use this wrapper, so the TLS handshake, request, and body
// read all happen through `fopen()` without any extra setup. Running this
// example requires network access.

$handle = fopen("https://example.com/", "r");
if ($handle === false) {
    echo "could not open the https:// stream (network access required)\n";
} else {
    $body = stream_get_contents($handle);
    fclose($handle);
    echo "fetched " . strlen($body) . " bytes over https://\n";
}
