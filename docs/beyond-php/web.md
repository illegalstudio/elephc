---
title: "Web Server (--web)"
description: "Compile a PHP program into a standalone prefork HTTP server binary with --web."
sidebar:
  order: 7
---

`--web` is an elephc compiler extension: it compiles a standard PHP file into a
standalone prefork HTTP server binary instead of a plain CLI executable. The PHP
source you compile is standard PHP — the same file would also run under the PHP
interpreter or php-fpm — but the compile-and-serve mechanism is specific to
elephc.

## Compiling a web server

```bash
elephc --web app.php
# app.php -> app  (a self-contained HTTP server binary)
```

The produced binary has no PHP runtime dependency. Run it with `--listen`:

```bash
./app --listen 127.0.0.1:8080
./app --listen 127.0.0.1:8080 --workers 4
```

## Runtime arguments

The produced binary accepts these arguments at runtime:

| Argument | Required | Default | Description |
|---|---|---|---|
| `--listen host:port` | Yes | — | Address and port to bind. Missing `--listen` prints an error to stderr and exits non-zero. |
| `--workers N` | No | CPU count | Number of worker processes to prefork. Minimum 1. |
| `--max-body-size N` | No | `8388608` (8 MiB) | Max request body in bytes; `0` means unlimited. A request whose body exceeds the cap gets `413 Payload Too Large` and the PHP handler never runs. |
| `--max-requests N` | No | `0` (never) | Recycle each worker after serving N requests (the master respawns it), bounding memory growth in long-running servers. |
| `--access-log` | No | off | Log one line per request to stderr (`<ip> "<method> <path>" <status> <ms>`). |
| `--help`, `--version` | No | — | Print usage / version and exit 0. |

## Request model

The request model follows PHP-FPM / `php -S`: each incoming HTTP request
re-runs the program's top-level code from a completely fresh state. Whatever
the script writes via `echo` or `print` becomes the HTTP response body. The
default response is `200 OK` with no `Content-Type` set; the program controls
the status and headers with `http_response_code()` and `header()` (see
[Response control](#response-control)).

```php
<?php
echo "Hello from elephc-web!";
```

Compiled with `--web`, the binary above serves `Hello from elephc-web!` for
every request.

See `examples/web-hello/` for a minimal runnable demo.

## Request input

The HTTP request is exposed through standard PHP superglobals, rebuilt fresh on
every request and readable inside any function scope (no `global` needed):

- **`$_SERVER`** — `REQUEST_METHOD`, `REQUEST_URI`, `QUERY_STRING`, the request
  headers as `HTTP_*` keys (e.g. `HTTP_USER_AGENT`), `CONTENT_TYPE` /
  `CONTENT_LENGTH` when present, plus `REMOTE_ADDR`, `REMOTE_PORT`, `SERVER_ADDR`,
  `SERVER_PORT`, `SERVER_NAME`, `SERVER_PROTOCOL`, `REQUEST_TIME`, `REQUEST_SCHEME`,
  `GATEWAY_INTERFACE`, and `SERVER_SOFTWARE`.
- **`$_GET`** — the query string parsed into a string-keyed array, percent-decoded.
- **`$_POST`** — an `application/x-www-form-urlencoded` request body parsed the
  same way; a `multipart/form-data` body also fills `$_POST` from its text fields.
  Other content types leave `$_POST` empty — read the raw body via `php://input`.
- **`$_FILES`** — `multipart/form-data` file uploads, each as
  `['name' => …, 'type' => …, 'tmp_name' => …, 'error' => 0, 'size' => …]`. The
  upload is written to a temp file at `tmp_name`; read it with
  `file_get_contents()` (or `move_uploaded_file()`).
- **`$_COOKIE`** — the `Cookie` request header parsed into a string-keyed array
  (values percent-decoded).
- **`$_REQUEST`** — `$_GET` overlaid with `$_POST` (POST wins on key collision),
  matching PHP's default `request_order = "GP"`.
- **`$_ENV`** — the process environment.
- **`php://input`** — `file_get_contents('php://input')` returns the raw request
  body (e.g. a JSON payload). An empty body returns `false`.

```php
<?php
echo "Hello, " . ($_GET['name'] ?? 'world') . "!\n";
if ($_SERVER['REQUEST_METHOD'] === 'POST') {
    echo "Raw body: " . file_get_contents('php://input') . "\n";
}
```

See `examples/web-request/` for a runnable demo covering `$_SERVER`, `$_GET`,
`$_POST`, and `php://input`.

## Response control

The response status and headers are controlled with the standard PHP builtins,
behaving as they do under PHP-FPM:

- **`http_response_code(int $code = 0): int`** — with a code, sets the response
  status and returns the previous code; with no argument (or `0`), returns the
  current status without changing it. The default status is `200`.
- **`header(string $header, bool $replace = true, int $response_code = 0): void`** —
  adds a response header. The argument is the full `"Name: Value"` line, exactly
  as in PHP:
  - By default (`$replace = true`) a later `header()` with the same field name
    replaces earlier ones; pass `$replace = false` to send duplicates.
  - A `"HTTP/1.1 404 ..."` or `"Status: 404 ..."` line sets the status code
    instead of adding a header.
  - A `"Location: ..."` header also sets the status to `302`, unless the status
    is already `201`/`3xx` or the third `$response_code` argument overrides it.
  - The third `$response_code` argument, when non-zero, forces the status.
- **`setcookie(...)` / `setrawcookie(...)`** — emit a `Set-Cookie` header (the
  classic positional signature `name, value, expires, path, domain, secure,
  httponly`). `setcookie()` percent-encodes the value; `setrawcookie()` does not.
  Multiple calls produce multiple `Set-Cookie` headers.

Unlike PHP-FPM, calling `header()` (or `setcookie()`) **after** producing output
is fine — elephc-web buffers the body and builds the response after the handler
returns, so there is no "headers already sent" error.

```php
<?php
header('Content-Type: application/json');
if (!isset($_GET['id'])) {
    http_response_code(400);
    echo '{"error":"missing id"}';
} else {
    echo '{"id":' . (int) $_GET['id'] . '}';
}
```

`Content-Type` is **not** set automatically — the program chooses it (PHP-FPM
defaults to `text/html`; elephc-web sets nothing unless you call `header()`).

See `examples/web-response/` for a runnable demo.

## A fuller example

`examples/web-framework/` builds a tiny Laravel-style framework on top of `--web`
— namespaced `Request`/`Response`/`Router` classes, single-action controllers
behind a `Handler` interface, a middleware onion (`Middleware` interface, e.g. an
API-key guard), `:param` route matching, and JSON responses — to show how the
pieces fit together in a real-ish application.

## Per-request fresh state

Between requests, the runtime resets all process-persistent state so request
N+1 sees the same clean environment request N did:

- **Global variables** — reset to their uninitialized state.
- **Function `static` variables** — released and zero-initialized; their
  initializers re-run on first call.
- **Static class properties** — released; their initializers re-run at the
  start of the handler body.

This matches PHP-FPM's per-request isolation model. No data leaks from one
request to the next.

## Concurrency model

The server uses a prefork model with `SO_REUSEPORT`: the master process forks N
worker processes before any request arrives, and the kernel load-balances
connections across workers.

Each worker is a separate process with its own copy of the runtime. Within a
single worker, requests are served **one at a time** — the PHP body runs to
completion before the next request is accepted. Parallelism equals the worker
count; a slow request occupies exactly one worker for its duration.

## Robustness

- **Graceful shutdown** — the master shuts down cleanly on `SIGINT` (Ctrl-C) and
  `SIGTERM`: it forwards termination to the workers, reaps them, and exits `0`. An
  in-flight request may be dropped when shutdown arrives.
- **Worker respawn** — a worker that dies unexpectedly (a crash, or PHP calling
  `exit()`) is replaced so the pool stays at `--workers` N.
- **Request body cap** — see `--max-body-size`; oversized bodies are rejected with
  `413` before the handler runs.
- **Slow-connection bound** — HTTP/1.1 keep-alive is enabled, but a connection that
  does not send the next request's headers within 30 s is closed. Because a worker
  serves one connection at a time, a kept-alive connection holds a worker until it
  closes or times out — size `--workers` accordingly.

## Limitations

The serve loop, per-request fresh state, request input (`$_SERVER` / `$_GET` /
`$_POST` / `$_COOKIE` / `$_REQUEST` / `$_ENV` / `$_FILES` / `php://input`), and
response control (`http_response_code()` / `header()` / `setcookie()`) are
available. The following are not yet available:

- **`$argc` / `$argv` not populated** — the binary's own argv is consumed by the
  server and is not forwarded to the script body (PHP-FPM does not set them either).
- **No intra-worker concurrency** — `handler()` runs synchronously, so one slow
  request occupies its worker until it completes (idle keep-alive connections no
  longer block the accept loop, but an in-flight handler does). Use `--workers`.
- **In-flight requests may drop on shutdown** — `SIGINT`/`SIGTERM` terminate
  workers promptly; there is no graceful connection drain yet.
- **No response streaming** — the whole body is buffered before it is sent.
- **`--listen` is TCP only** — Unix-domain-socket listening is not yet supported.
- **No sessions** — `$_SESSION` / `session_start()` are not provided. Cookies
  (`$_COOKIE`, `setcookie()`) are, so you can build session handling yourself.
- **Not supported in this release:** sessions, static file serving, in-process
  TLS, HTTP/2–3 — front the server with a reverse proxy for these (below).

## Behind a reverse proxy

elephc-web speaks HTTP/1.1 in cleartext only. For TLS, HTTP/2/3, static asset
serving, or virtual hosting, run it behind a reverse proxy (nginx, Caddy,
HAProxy) that terminates TLS and forwards to `--listen`. A typical setup binds
the server to `127.0.0.1:8080` and points the proxy at it.

## Mutual exclusions

`--web` cannot be combined with `--check`, `--emit cdylib`, `--emit-asm`, or
`--emit-ir`.
