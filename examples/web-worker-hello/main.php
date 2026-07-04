<?php
// Worker-mode demo: boot an "app" once (set up a static cache), register a
// request handler that reads from the cache and the per-request $_GET
// superglobal, and serve requests.
//
// Unlike classic `--web` (which re-runs the whole top level per request), the
// top-level boot here runs exactly once per worker. The static cache persists
// across requests, so the "warm-up" cost is paid once. Per-request
// superglobals ($_SERVER/$_GET/$_POST/$_COOKIE/$_REQUEST/$_FILES/$_ENV) are
// populated fresh on each request by the trampoline's fill functions.
class Cache {
    public static string $greeting = '';
}

// Boot: populate the cache once when the worker starts. This work is NOT
// repeated per request.
Cache::$greeting = 'Hello';

elephc_worker_register(function () {
    // Each request reads the persistent cache and the per-request $_GET
    // superglobal (populated by the trampoline's fill functions), and echoes
    // the greeting plus a per-worker counter that increments across requests
    // (demonstrating state persistence within a worker).
    static $calls = 0;
    $calls++;
    echo Cache::$greeting . ', ' . ($_GET['name'] ?? 'world') . ' (call ' . $calls . ')';
});
