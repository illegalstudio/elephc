---
title: "Web Server (--web)"
description: "Compile a PHP program into a standalone HTTP server binary with --web."
sidebar:
  order: 7
---

`--web` is an elephc compiler extension: it compiles a standard PHP file into a
standalone HTTP server binary instead of a plain CLI executable. Unix targets use
a prefork supervisor; Windows uses a single-process event loop. The PHP
source you compile is standard PHP — the same file would also run under the PHP
interpreter or php-fpm — but the compile-and-serve mechanism is specific to
elephc.

## Compiling a web server

```bash
elephc --web app.php
# app.php -> app  (a self-contained HTTP server binary)
```

Plain `--web` selects the original high-throughput in-process worker model. The
isolation model is a **compile-time** choice, not a runtime server switch:

```bash
elephc --web --web-isolation=worker app.php   # same as plain --web
elephc --web --web-isolation=pool app.php     # persistent handler pool
elephc --web --web-isolation=request app.php  # disposable child per request
```

The generated process entry calls a different bridge symbol for each model, so
the default worker request path does not pay an isolation branch or IPC cost.
`--web-isolation` without `--web` (or its `--with-web` alias) is an error.

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
| `--max-body-size N` | No | `8388608` (8 MiB) | Max request body in bytes. A request whose body exceeds the cap gets `413 Payload Too Large` and the PHP handler never runs. `0` removes the cap in `worker` only: `pool` and `request` hand the body to a separate handler process over the broker protocol, which refuses anything above its own 64 MiB frame limit with the same `413`. |
| `--max-requests N` | No | `0` (never) | Recycle each worker after N completed requests. It stops accepting, drains active HTTP connections, and exits after isolated handlers/brokers are reaped; the master then respawns it. |
| `--access-log` | No | off | Log one line per request to stderr (`<ip> "<method> <path>" <status> <ms>`). |
| `--max-execution-time N` | No | `0` (none) | In `worker`, terminate and respawn the worker. In `pool`/`request`, terminate only the timed-out handler process. |
| `--handler-concurrency N` | No | `1` | Handler processes per web worker. Available only in `pool` and `request`. |
| `--max-handler-requests N` | No | `1000` | Requests served by one persistent handler before replacement; `0` disables recycling. Available only in `pool`. |
| `--body-read-timeout N` | No | `30` | Seconds allowed to receive a request body; `0` means unlimited. Enforced in every isolation mode, including the default `worker`. Expiry answers `408 Request Timeout` and closes the connection. |
| `--response-write-timeout N` | No | `30` | Seconds an isolated response may remain blocked by client backpressure; `0` means unlimited. Available only in `pool` and `request`. |
| `--gzip` | No | off | Compress responses when the client sends `Accept-Encoding: gzip`. |
| `--help`, `--version` | No | — | Print usage / version and exit 0. |

Mode-specific runtime flags are rejected by binaries that cannot implement
them; they are never silently ignored.

## HTTP request lifecycle

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

In the default `worker` model, calling `header()` (or `setcookie()`) **after**
producing output is accepted because the whole response is buffered until the
handler returns. In `pool` and `request`, output streams immediately: the first
body byte commits status and headers, and later header/status mutations are
ignored with PHP-style "headers already sent" diagnostics.

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

Between requests, the generated PHP runtime resets its request-visible state so
request N+1 sees the same PHP environment request N did:

- **Global variables** — reset to their uninitialized state.
- **Function `static` variables** — released and zero-initialized; their
  initializers re-run on first call.
- **Static class properties** — released; their initializers re-run at the
  start of the handler body.

This matches PHP-FPM for PHP globals and statics in all three models. Native
bridge state that intentionally lives outside the PHP heap follows the selected
process lifetime; see [Concurrency model](#concurrency-model).

## Concurrency model

All models start with a prefork master and `--workers N` `SO_REUSEPORT` web
workers. The compile-time isolation model decides where PHP executes:

| Model | PHP process lifetime | Maximum parallel handlers | Response | Operational trade-off |
|---|---|---:|---|---|
| `worker` (default) | The web worker itself | `workers` | Buffered | Original main performance and process-global persistence; a crash/timeout recycles the worker. |
| `pool` | Persistent supervised handler children | `workers × handler-concurrency` | Streaming | No per-request fork; bridge/process state persists inside each pool child. A crashed, cancelled, timed-out, or quota-retired child is reaped and replaced. |
| `request` | One disposable child per request | `workers × handler-concurrency` | Streaming | Strongest process-global isolation, with fork/COW/page-fault overhead on every request. |

### Choosing a model

If in doubt, start with `worker`. All three models reset PHP globals, function
statics, and static properties between requests; selecting `request` is necessary
only when state outside that PHP heap must also disappear after every response.

- **Typical JSON API or dashboard — `worker`.** Use this for trusted application
  code when throughput and latency matter most. Add web workers to increase PHP
  parallelism. A handler crash or execution timeout replaces the whole web worker.
- **PDO/FFI application needing containment — `pool`.** Persistent children avoid
  the fork-per-request cost, a faulty handler is replaced without killing its web
  worker, and `--handler-concurrency` allows several PHP handlers below each web
  worker. PDO and other native caches are per pool child: they can be reused, but
  requests have no affinity to a particular child.
- **Fragile native integration or strict process reset — `request`.** Use this when
  a request may leave unwanted state in a native library, allocator, FFI dependency,
  or other process-global bridge and that state must never reach the next request.
  Every request pays the fork/COW/page-fault cost, and persistent PDO connections
  cannot survive, so this should not be the general-purpose performance choice.

Example deployments:

```bash
# Fast stateless API: four independent in-process PHP workers.
elephc --web api.php
./api --listen 0.0.0.0:8080 --workers 4 --max-execution-time 30

# Database-backed service: two web workers, four persistent handlers each
# (at most eight concurrent PHP handlers), recycled after 1,000 requests.
elephc --web --web-isolation=pool service.php
./service --listen 0.0.0.0:8080 --workers 2 \
  --handler-concurrency 4 --max-handler-requests 1000 --max-execution-time 30

# Native/FFI endpoint: every completed request discards its handler process.
elephc --web --web-isolation=request native-endpoint.php
./native-endpoint --listen 127.0.0.1:8080 --workers 2 \
  --handler-concurrency 2 --max-execution-time 10
```

`pool` and `request` add one threadless broker below each web worker. The broker
owns every handler PID. Dispatches have unique IDs; client disconnects and
response-write timeouts cancel the exact live or queued request, and shutdown
terminates and reaps the complete descendant tree. Descriptor handoffs are
acknowledged before the sender closes its copy. A broker failure exits its web
worker so the master reconstructs a clean worker/broker pair.

## Robustness

- **Graceful shutdown** — the master shuts down cleanly on `SIGINT` (Ctrl-C) and
  `SIGTERM`: it forwards termination to the workers, reaps them, and exits `0`. An
  in-flight request may be dropped when shutdown arrives.
- **Process supervision** — the master replaces failed web workers. In isolated
  modes, the broker separately tracks, reaps, and replaces handler children.
- **Planned recycling** — `--max-requests` stops new accepts at the quota, asks
  keep-alive connections to close after their current responses, drains in-flight
  work, and reaps the isolated broker before the worker exits for replacement.
- **Request body cap** — see `--max-body-size`; oversized bodies are rejected with
  `413` before the handler runs.
- **Slow-connection bound** — HTTP/1.1 keep-alive is enabled, but a connection that
  does not send the next request's headers within 30 s is closed. Isolated modes
  additionally bound body reads and stalled response writes by default.

## Sessions

Sessions are available under `--web` via PHP's standard session functions,
providing persistent state across HTTP requests.

- `session_start()` initializes the session, populates the `$_SESSION`
  superglobal, and sends a `Set-Cookie: PHPSESSID=...` header on the first
  request (when no session cookie is present).
- Data persists across requests via a session cookie (`PHPSESSID` by default).
  Subsequent requests carry the cookie and `session_start()` loads the matching
  session file.
- Session data is stored in files, matching PHP's default
  `session.save_handler = files`.
- The configured save path defaults to an empty string, which the files handler
  resolves to `sys_get_temp_dir()`. It also accepts php-src's
  `[depth;[mode;]]path` sharding grammar.
- `session_write_close()` is called automatically at handler end via a finally
  block, so session data is flushed even if the handler exits early or throws.
- File locking (`flock`) prevents concurrent requests from overwriting each
  other's session data.
- Custom object and legacy callable save handlers are supported alongside the
  built-in files handler, including strict-ID and lazy-timestamp interfaces.
- Serialized payloads use a binary-safe pointer/length bridge, so embedded NUL
  bytes and all three PHP session serializers round-trip exactly.

```php
<?php
session_start();

if (!isset($_SESSION['count'])) {
    $_SESSION['count'] = 0;
}
$_SESSION['count']++;

header('Content-Type: text/plain');
echo "Visits: " . $_SESSION['count'] . "\n";
echo "Session ID: " . session_id() . "\n";
```

See `examples/web-session/` for a runnable demo, and [Sessions](../php/sessions.md)
for the full function reference.

## Limitations

The serve loop, per-request fresh state, request input (`$_SERVER` / `$_GET` /
`$_POST` / `$_COOKIE` / `$_REQUEST` / `$_ENV` / `$_FILES` / `php://input`),
response control (`http_response_code()` / `header()` / `setcookie()`), and
sessions (`session_start()` / `$_SESSION` / `session_write_close()`) are
available. The following are not yet available:

- **`$argc` / `$argv` not populated** — the binary's own argv is consumed by the
  server and is not forwarded to the script body (PHP-FPM does not set them either).
- **Default worker mode has no intra-worker concurrency** — `handler()` runs
  synchronously. Use more workers or compile with `pool`/`request` and set
  `--handler-concurrency`.
- **In-flight requests may drop on shutdown** — `SIGINT`/`SIGTERM` terminate
  workers promptly; there is no graceful connection drain yet.
- **Worker mode does not stream responses** — `pool` and `request` do.
- **Execution is intentionally unbounded when `--max-execution-time 0`** — a
  handler that never returns keeps its process slot while its client remains
  connected. Client disconnect and response-write timeout cancellation still
  reclaim isolated handlers.
- **`--listen` is TCP only** — Unix-domain-socket listening is not yet supported.
- **Not supported in this release:** static file serving, in-process
  TLS, HTTP/2–3 — front the server with a reverse proxy for these (below).

## Behind a reverse proxy

elephc-web speaks HTTP/1.1 in cleartext only. For TLS, HTTP/2/3, static asset
serving, or virtual hosting, run it behind a reverse proxy (nginx, Caddy,
HAProxy) that terminates TLS and forwards to `--listen`. A typical setup binds
the server to `127.0.0.1:8080` and points the proxy at it.

## Mutual exclusions

`--web` cannot be combined with `--check`, `--emit cdylib`, `--emit-asm`, or
`--emit-ir`.
