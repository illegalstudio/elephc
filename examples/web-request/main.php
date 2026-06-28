<?php
// elephc-web Phase 2: request input via standard PHP superglobals.
//
// Compile: cargo run -- --web examples/web-request/main.php
// Run:     ./examples/web-request/main --listen 127.0.0.1:8080
// Try:     curl 'http://127.0.0.1:8080/hello?name=ada'
//          curl -d 'msg=hi' http://127.0.0.1:8080/
//          curl -d '{"x":1}' -H 'Content-Type: application/json' http://127.0.0.1:8080/
//
// Each request re-runs this script from a fresh state; $_SERVER/$_GET/$_POST are
// rebuilt per request and are readable inside any function. Whatever you echo
// becomes the response body.

echo "Method: " . $_SERVER['REQUEST_METHOD'] . "\n";
echo "URI:    " . $_SERVER['REQUEST_URI'] . "\n";

if (isset($_GET['name'])) {
    echo "Hello, " . $_GET['name'] . "!\n";
}

if (isset($_POST['msg'])) {
    echo "You said: " . $_POST['msg'] . "\n";
}

$raw = file_get_contents('php://input');
if ($raw !== false && $raw !== '') {
    echo "Raw body: " . $raw . "\n";
}
