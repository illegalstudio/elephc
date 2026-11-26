<?php
// PHP 8.6 external-PSK TLS client.
//
// Start a certificate-less TLS 1.3 PSK server (the PSK below is "secret"):
//   openssl s_server -accept 9443 -tls1_3 \
//     -psk 736563726574 -psk_identity client -nocert -www
//
// Build and run:
//   elephc build examples/tls-psk/main.php --php-version 8.6
//   ./examples/tls-psk/main https://127.0.0.1:9443/
//
// `ssl.psk_client_cb` selects the libssl-backed TLS bridge; the manifest
// declares the managed openssl-crypto package it links against.
$url = $argv[1] ?? "https://127.0.0.1:9443/";
$context = stream_context_create([
    "ssl" => [
        "verify_peer" => "0",
        "psk_client_cb" => function () {
            return new Openssl\Psk("secret", "client");
        },
    ],
]);

echo file_get_contents($url, false, $context);
