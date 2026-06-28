<?php
// elephc-web demo — a standalone, compiled HTTP server.
//
// Compile:  cargo run -- --web examples/web-demo/main.php
// Run:      ./examples/web-demo/main --listen 127.0.0.1:8080 --workers 4
// Request:  curl http://127.0.0.1:8080/
//
// Each HTTP request re-runs this whole script from a fresh state; whatever it
// echoes becomes the response body. Phase 1 is body-only: there is no request
// input ($_GET/$_POST/$_SERVER) or custom headers/status yet.

function triangle(int $rows): string {
    $out = "";
    for ($r = 1; $r <= $rows; $r++) {
        for ($c = 0; $c < $r; $c++) {
            $out .= "*";
        }
        $out .= "\n";
    }
    return $out;
}

echo "Served by elephc-web: compiled PHP, no interpreter, no VM.\n";
echo "\n";
echo triangle(6);
echo "\n";

$sum = 0;
for ($i = 1; $i <= 100; $i++) {
    $sum += $i;
}
echo "1 + 2 + ... + 100 = " . $sum . "\n";
