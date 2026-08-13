<?php
// curl_* — a GET request through libcurl, with error handling. curl_init()/curl_setopt()/
// curl_exec()/curl_getinfo()/curl_errno()/curl_error()/curl_close() are backed by elephc's
// own statically pinned libcurl 8.21.0 (crates/elephc-curl), never a system libcurl.
// Running this example for real requires network access; like examples/http-stream/main.php
// it still compiles and reports why nothing was fetched when the network is unavailable,
// rather than crashing.
//
// This landing's protocol matrix includes https://, but the managed curl/OpenSSL recipe
// does not yet bundle a CA trust store (no --with-ca-bundle/--with-ca-path), so an https://
// transfer against a real certificate is not something this landing proves — it depends on
// whatever CA paths happen to exist on the machine running the compiled binary. The
// headline request below stays on plain http:// on purpose; HTTPS arrives with Task 8's
// TLS wave, which is also where this example gets an https:// counterpart.

$ch = curl_init("http://example.com/");
curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);

$body = curl_exec($ch);

if ($body === false) {
    echo "GET http://example.com/ failed: errno " . curl_errno($ch) . " (" . curl_error($ch) . ")\n";
} else {
    // CURLINFO_HTTP_CODE is the one curl_getinfo() option elephc understands today
    // (Task 8 adds the rest of the CURLINFO_* surface).
    $status = curl_getinfo($ch, CURLINFO_HTTP_CODE);
    echo "GET http://example.com/ -> HTTP " . $status . ", " . strlen($body) . " bytes\n";
}

// curl_close() is a no-op in PHP 8: curl sessions are CurlHandle OBJECTS, not resources, so
// the handle stays usable afterwards and frees its native libcurl handle only when it goes
// out of scope.
curl_close($ch);
echo ($ch instanceof CurlHandle) ? "handle still usable after curl_close()\n" : "unexpected\n";

// The failure shape needs no network at all: a closed local port demonstrates curl_exec()
// returning false with a real libcurl errno/error pair.
$refused = curl_init("http://127.0.0.1:1/");
curl_setopt($refused, CURLOPT_RETURNTRANSFER, true);
$result = curl_exec($refused);
echo $result === false
    ? "connection refused, as expected (errno " . curl_errno($refused) . "): " . curl_error($refused) . "\n"
    : "unexpectedly succeeded\n";
