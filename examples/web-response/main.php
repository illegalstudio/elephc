<?php
// elephc-web Phase 3: response control (status + headers).
// Compile: cargo run -- --web examples/web-response/main.php
// Run:     ./examples/web-response/main --listen 127.0.0.1:8080
// Try:     curl -i 'http://127.0.0.1:8080/?name=ada'    (200 + greeting)
//          curl -i 'http://127.0.0.1:8080/'             (400)

header('Content-Type: text/plain; charset=utf-8');

if (!isset($_GET['name'])) {
    http_response_code(400);
    echo "missing 'name' query parameter\n";
} else {
    http_response_code(200);
    header('X-Powered-By: elephc');
    echo "Hello, " . $_GET['name'] . "!\n";
}
