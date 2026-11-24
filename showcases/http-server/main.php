<?php
// elephc http-server — a native, asynchronous HTTP/1.1 server.
//
// Build & run:
//   cargo run -- build showcases/http-server/main.php
//   ./showcases/http-server/main
//
// Then open http://127.0.0.1:8080/ in a browser, or:
//   curl http://127.0.0.1:8080/hello?name=elephc

require_once 'native.php';
require_once 'http.php';
require_once 'routes.php';
require_once 'server.php';

run_http_server(8080);
