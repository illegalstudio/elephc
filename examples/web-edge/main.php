<?php

// Minimal Ferron/edge deployment example. Ferron terminates TLS and serves
// /static; every other request reaches this compiled origin over HTTP/1.1.
if (($_SERVER['REQUEST_URI'] ?? '') === '/healthz') {
    header('Content-Type: text/plain');
    echo "ok\n";
} else {
    $savePath = (string) getenv('ELEPHC_SESSION_SAVE_PATH');
    if ($savePath !== '') {
        session_save_path($savePath);
    }
    session_start();
    $_SESSION['count'] = ($_SESSION['count'] ?? 0) + 1;

    header('Content-Type: text/plain');
    echo 'REMOTE_ADDR=', $_SERVER['REMOTE_ADDR'] ?? '<absent>', "\n";
    echo 'REMOTE_PORT=', $_SERVER['REMOTE_PORT'] ?? '<absent>', "\n";
    echo 'REQUEST_SCHEME=', $_SERVER['REQUEST_SCHEME'] ?? '<absent>', "\n";
    echo 'HTTPS=', $_SERVER['HTTPS'] ?? '<absent>', "\n";
    echo 'SERVER_PORT=', $_SERVER['SERVER_PORT'] ?? '<absent>', "\n";
    echo 'HTTP_HOST=', $_SERVER['HTTP_HOST'] ?? '<absent>', "\n";
    echo 'session=', $_SESSION['count'], "\n";
}
