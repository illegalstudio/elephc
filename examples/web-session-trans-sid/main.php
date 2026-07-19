<?php
// elephc-web: session.use_trans_sid — propagate the session id without cookies.
//
// Compile: cargo run -- --web examples/web-session-trans-sid/main.php
// Run:     ./examples/web-session-trans-sid/main --listen 127.0.0.1:8080
// Try (no cookie jar, so URL rewriting kicks in):
//          curl -s 127.0.0.1:8080/          # links/forms get ?PHPSESSID=<id>
//          curl -si 127.0.0.1:8080/go       # Location: header rewritten too
// Try (with a cookie jar, so rewriting stays OFF — the cookie carries the id):
//          curl -s -c jar -b jar 127.0.0.1:8080/
//
// How it works
// ------------
// When a request arrives WITHOUT the session cookie, elephc can carry the
// session id through the HTML itself instead. This rewriting activates only
// when all of these hold:
//   - session.use_trans_sid  = 1
//   - session.use_only_cookies = 0   (its default 1 disables URL propagation)
//   - a session is active with a non-empty id
//   - the request did NOT already send the session cookie
// The rewrite runs on the buffered response before it is sent: same-origin
// href/src attributes get ?PHPSESSID=<id> appended, <form> tags get a hidden
// PHPSESSID field injected, and a same-origin Location: header is rewritten too.
// Off-host URLs, mailto:/javascript:, and fragment-only links are left alone, so
// the id never leaks to third parties.

// Opt into URL propagation for this demo (both flags are required).
session_start([
    'use_trans_sid'    => 1,
    'use_only_cookies' => 0,
]);

$path = $_SERVER['REQUEST_URI'];
$q = strpos($path, '?');
if ($q !== false) {
    $path = substr($path, 0, $q);
}

if ($path === '/go') {
    // A redirect whose same-origin Location gets the session id appended.
    http_response_code(302);
    header('Location: /landed');
    echo "redirecting...\n";
} elseif ($path === '/landed') {
    header('Content-Type: text/plain; charset=utf-8');
    echo "You arrived with session id " . session_id() . "\n";
} else {
    header('Content-Type: text/html; charset=utf-8');
    // None of these URLs carry PHPSESSID in the source. On a cookie-less
    // request elephc rewrites the same-origin ones; the external link and the
    // mailto: are deliberately left untouched.
    echo '<!doctype html><html><body>';
    echo '<p>Session id: ' . session_id() . '</p>';
    echo '<a href="/landed">same-origin link (rewritten)</a><br>';
    echo '<a href="https://example.com/">external link (untouched)</a><br>';
    echo '<a href="mailto:hi@example.com">mailto (untouched)</a><br>';
    echo '<form action="/landed" method="get">';
    echo '<button type="submit">same-origin form (hidden field injected)</button>';
    echo '</form>';
    echo '</body></html>';
}
