<?php
// elephc-web: a tiny method + path router.
// Compile: cargo run -- --web examples/web-router/main.php
// Run:     ./examples/web-router/main --listen 127.0.0.1:8080 --access-log
// Try:     curl -i 127.0.0.1:8080/
//          curl -i '127.0.0.1:8080/hello?name=ada'
//          curl -i -X POST -d 'msg=hi' 127.0.0.1:8080/echo
//          curl -i 127.0.0.1:8080/whoami        (shows REMOTE_ADDR)
//          curl -i 127.0.0.1:8080/missing       (404)

header('Content-Type: text/plain; charset=utf-8');

$method = $_SERVER['REQUEST_METHOD'];
$path = $_SERVER['REQUEST_URI'];
// Strip the query string from the path for routing.
$q = strpos($path, '?');
if ($q !== false) {
    $path = substr($path, 0, $q);
}

if ($method === 'GET' && $path === '/') {
    echo "elephc-web router\n";
    echo "routes: /, /hello, /echo (POST), /whoami\n";
} elseif ($method === 'GET' && $path === '/hello') {
    $name = $_GET['name'] ?? 'world';
    echo "Hello, " . $name . "!\n";
} elseif ($method === 'POST' && $path === '/echo') {
    echo "you sent: " . ($_POST['msg'] ?? '(no msg field)') . "\n";
} elseif ($method === 'GET' && $path === '/whoami') {
    echo "you are " . $_SERVER['REMOTE_ADDR'] . ":" . $_SERVER['REMOTE_PORT'] . "\n";
    echo "served by " . $_SERVER['SERVER_SOFTWARE'] . " over " . $_SERVER['SERVER_PROTOCOL'] . "\n";
} else {
    http_response_code(404);
    echo "404 Not Found: " . $method . " " . $path . "\n";
}
