<?php
// PHP 8.6 TLS session resumption and client 0-RTT.
//
// Capture a session from the server once:
//   openssl s_client -connect example.com:443 -sess_out session.pem </dev/null
//
// Then build and run:
//   elephc build examples/tls-session-resume/main.php --php-version 8.6
//   ./examples/tls-session-resume/main session.pem
//
// `ssl.session_data` selects the libssl-backed TLS bridge; the project manifest
// declares the managed openssl-crypto package it links against. The session must
// advertise early-data support for `ssl.early_data` to be sent as 0-RTT.
$path = $argv[1] ?? "session.pem";
$session = Openssl\Session::import(file_get_contents($path));

printf(
    "session %s, protocol %s, cipher %s\n",
    bin2hex($session->id),
    $session->getProtocol(),
    $session->getCipher(),
);

$captured = null;
$context = stream_context_create([
    "ssl" => [
        "session_data" => $session,
        "early_data" => "GET / HTTP/1.0\r\n\r\n",
        "session_new_cb" => function ($stream, $newSession) use (&$captured) {
            $captured = $newSession;
        },
    ],
]);

$stream = fopen("https://example.com/", "r", false, $context);
$meta = stream_get_meta_data($stream);
echo "early data: ", $meta["crypto"]["early_data"] ?? "not attempted", "\n";
echo stream_get_contents($stream);
fclose($stream);
echo "new session: ", $captured === null ? "none" : $captured->getProtocol(), "\n";
