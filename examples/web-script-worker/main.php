<?php
// Non-intrusive worker mode demo (`--web-worker=script`).
//
// Compile & serve with elephc:
//   elephc --web-worker=script examples/web-script-worker/main.php
//   ./examples/web-script-worker/main --listen 127.0.0.1:8080
//
// The SAME unmodified file also runs under php-fpm / the PHP dev server:
//   php -S 127.0.0.1:8080 examples/web-script-worker/main.php
//
// This is 100% standard PHP: no elephc_worker_register, no elephc-only syntax.
// Under `--web-worker=script` the whole top level re-runs on every request, but
// function `static` locals persist across requests within a worker — so the
// boot-once null-guard in container() below builds the "container" exactly once
// per worker and reuses it thereafter. Under classic `--web` (or php-fpm) the
// static resets each request, so the same code simply rebuilds every time
// (correct, just slower).

/**
 * Build the app's read-only "container" (config, warm caches, etc.).
 * The heavy work you only want to pay for once per worker lives here.
 */
function build_container(): array
{
    return [
        'app'      => 'web-script-worker',
        'built_at' => date('H:i:s'),
    ];
}

/**
 * The canonical boot-once null-guard. `$c` persists across requests under
 * script mode, so build_container() runs on request 1 and the same container
 * is reused on every later request. Under classic `--web` / php-fpm the static
 * resets per request, so it rebuilds each time.
 */
function container(): array
{
    static $c = null;
    if ($c === null) {
        $c = build_container();
    }
    return $c;
}

/**
 * Per-worker request counter: a function `static` that survives across
 * requests, incremented once per request to make persistence visible.
 */
function request_number(): int
{
    static $n = 0;
    return ++$n;
}

$app  = container();
$n    = request_number();
$name = $_GET['name'] ?? 'world'; // fresh per request (superglobal)

header('Content-Type: text/plain');
echo "app:      {$app['app']}\n";
echo "built_at: {$app['built_at']}  (constant across requests in this worker)\n";
echo "request:  #{$n}  (increments across requests in this worker)\n";
echo "hello:    {$name}  (fresh per request from \$_GET)\n";
