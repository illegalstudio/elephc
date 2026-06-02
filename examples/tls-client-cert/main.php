<?php

// TLS client certificates (mutual TLS) via stream context options.
//
// Set ssl.local_cert (PEM certificate chain) and ssl.local_pk (unencrypted PEM
// private key) on a stream context, then promote a connected TCP socket to TLS
// with stream_socket_enable_crypto(). When both options are present, elephc
// presents the client certificate during the handshake (rustls with_client_auth_cert).
//
// Real usage against a server that requests a client certificate:
//
//     $ctx = stream_context_create(['ssl' => [
//         'local_cert' => '/path/to/client-cert.pem',
//         'local_pk'   => '/path/to/client-key.pem',
//         'peer_name'  => 'api.example.com',
//     ]]);
//     $sock = stream_socket_client('tcp://api.example.com:443');
//     stream_socket_enable_crypto($sock, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
//
// This self-contained demo points local_cert/local_pk at paths that do not
// exist, so the client-auth config load fails before any network I/O and
// enable_crypto() reports false — demonstrating the graceful failure path
// without needing a client-auth-requiring server.
//
// Notes / limitations:
//   - The private key must be unencrypted. ssl.passphrase is not honored.
//   - ssl.ciphers and ssl.security_level are accepted but not honored: rustls
//     uses a fixed set of modern cipher suites and negotiates TLS 1.2/1.3.

$ctx = stream_context_create([
    'ssl' => [
        'local_cert'     => '/path/to/missing-client-cert.pem',
        'local_pk'       => '/path/to/missing-client-key.pem',
        'ciphers'        => 'ECDHE-RSA-AES128-GCM-SHA256', // accepted, not honored
        'security_level' => 2,                             // accepted, not honored
    ],
]);

$stream = fopen('php://memory', 'r+');
$ok = stream_socket_enable_crypto($stream, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
fclose($stream);

echo $ok ? "client-cert TLS enabled\n" : "client-cert load failed (expected for this demo)\n";
