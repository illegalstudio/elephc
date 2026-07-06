---
title: "Web Server (--web / --web-worker)"
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
| `--max-requests N` | No | `0` (never) | Recycle each **worker process** after serving N requests (the master respawns it), bounding memory growth in long-running servers. Do not confuse with `--max-requests-per-connection`. |
| `--max-requests-per-connection N` | No | `0` (opt-in) | Close a keep-alive **connection** after N responses by sending `Connection: close`, so the client reconnects and `SO_REUSEPORT` re-picks a worker (see [Keep-alive and load distribution](#keep-alive-and-load-distribution-across-workers)); `0` = unlimited (off by default; no behavior change from before this flag existed). Same default in all three web modes. |
| `--idle-timeout SECS` | No | `0` (opt-in) | Close a keep-alive connection that stays idle (no new request) for more than SECS seconds, so the client reconnects; `0` = never (off by default; no behavior change from before this flag existed). Same default in all three web modes. |
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

Only the superglobals your program actually references are built each request:
the compiler detects which of `$_SERVER`, `$_GET`, `$_POST`, `$_FILES`,
`$_COOKIE`, `$_REQUEST`, and `$_ENV` appear in the program (including inside
included and autoloaded files) and skips the per-request work for the rest. This
is transparent — a superglobal you never read is one you cannot observe — so it
only saves time; a program that reads all of them behaves exactly as before.
Superglobals that depend on others are pulled in automatically (`$_REQUEST` builds
`$_GET` and `$_POST`; `$_POST` and `$_COOKIE` build `$_SERVER`).

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

- **User `global` variables** — released and zero-initialized, so a global
  written only inside a function (or conditionally) does not carry over.
- **Function `static` variables** — released and zero-initialized; their
  initializers re-run on first call.
- **Static class properties** — released; their initializers re-run at the
  start of the handler body.
- **Superglobals** — released and rebuilt fresh from the incoming request.

This matches PHP-FPM's per-request isolation model. No data leaks from one
request to the next.

## Worker mode (`--web-worker`)

`--web-worker` is an alternative to `--web` for long-lived applications. Instead
of re-running the program's top level per request (the PHP-FPM model), the
top-level **boots once** per worker process, registers a request handler, and
the Rust runtime drives the HTTP accept loop — invoking the registered handler
for each request. This is the FrankenPHP / RoadRunner-style model.

```bash
elephc --web-worker app.php
./app --listen 127.0.0.1:8080 --workers 4
```

### The API

A worker-mode program registers exactly one handler with
`elephc_worker_register`:

```php
<?php
elephc_worker_register(function () {
    echo "Hello from worker!";
});
```

`elephc_worker_register(callable $handler): void` takes a single callable (a
closure, named function, invokable object, or first-class callable syntax).
Calling it transfers control to the Rust worker loop; any code after the call
is unreachable. Exactly one handler must be registered per worker; registering
more than once replaces the handler.

Whatever the handler writes via `echo` / `print` becomes the HTTP response
body, exactly as in classic `--web`. Response control (`http_response_code()`,
`header()`, `setcookie()`) works identically.

### State persistence

Within a single worker process, persistent state **survives across requests**:

- **Function `static` locals** — retain their value across requests.
- **Static class properties** — retain their value across requests.
- **Global variables** — retain their value across requests.
- **`$_ENV`** — read from the process environment **once at boot** and kept for
  the worker's lifetime. The environment is fixed at fork, so re-reading it per
  request would be wasted work; a mutation to `$_ENV` during a request therefore
  persists into the next request in this mode (unlike classic `--web`). Read
  `getenv()` if you need the live process environment.

This is the opposite of classic `--web`, which resets all of the above per
request. A boot-heavy application (framework bootstrap, DI container build,
config parse, database connection pool warmup) pays that cost once per worker
instead of once per request.

Request-scoped state still resets per request:

- **`$_SERVER`, `$_GET`, `$_POST`, `$_COOKIE`, `$_REQUEST`, `$_FILES`** — are
  rebuilt fresh per request (same as classic `--web`).
- **`php://input`** — returns the current request's raw body.
- **`$argc` / `$argv`** — not populated (as in classic `--web`).

### Lifecycle

1. **Boot** (once per worker): the top-level PHP runs, initializing the
   application (build the DI container, open connections, populate caches),
   then calls `elephc_worker_register($handler)`.
2. **Register**: the callable is stored; control transfers to the Rust worker
   loop (`elephc_worker_register` never returns to PHP).
3. **Per request**: the Rust loop accepts a connection, populates the request
   superglobals and response state, invokes the registered handler, captures
   its output, builds the HTTP response, sends it, then cleans up multipart
   temp files and runs the cycle collector.
4. **Recycle**: when the worker has served `--max-requests` requests, it exits
   cleanly and the master forks a fresh one (re-running the boot).

### GC

After each handler invocation the runtime cycle collector
(`__rt_gc_collect_cycles`) runs, gated by `--worker-gc-interval`, to reclaim
cyclic garbage that plain refcounting cannot free — while keeping the
persistent statics and globals alive. The default cadence in worker mode is
`1` (collect after every request).

Each request releases and rebuilds the request superglobals (a handful of hash
allocations per request), and the default `--worker-gc-interval` of `1` runs
the cycle collector on every request — raise the interval to trade peak
memory for lower per-request latency.

A request that ends by unwinding — an `exit()`, `die()`, or an uncaught `throw`
out of a function/method — releases the owned refcounted locals (strings,
arrays, objects) of every frame it unwinds through, not just the top-level body.
Each function emits a per-frame cleanup callback that the unwinder invokes as it
walks back to the request boundary, so aborting from deep in the call stack
while holding a large working set does not leak across requests.

### Worker-mode runtime arguments

The `--web-worker` binary accepts the same runtime arguments as `--web`, plus
two worker-mode defaults:

| Argument | Default (worker mode) | Description |
|---|---|---|
| `--max-requests N` | `1000` | Recycle each worker after N requests (bounds memory growth; classic `--web` defaults to `0` = never). |
| `--worker-gc-interval N` | `1` | Run the cycle collector every N requests (`0` = never, `1` = every request). |

All other runtime arguments (`--listen`, `--workers`, `--max-body-size`,
`--max-requests-per-connection`, `--idle-timeout`, `--max-execution-time`,
`--access-log`, `--gzip`, `--help`, `--version`) behave the same as in classic
`--web`. In particular the keep-alive rotation flags
(`--max-requests-per-connection`, `--idle-timeout`) keep their `0` / `0`
(opt-in, off) defaults in worker mode — unlike `--max-requests`, they do not
vary by mode.

### When to use worker mode

Use `--web-worker` when the per-request boot cost dominates — typically
frameworks with large DI containers, applications that open long-lived
connections (database, Redis) at startup, or apps that warm caches once. Use
classic `--web` when per-request isolation matters more than boot cost, or
when the program is simple enough that boot is negligible.

### Migrating from classic `--web`

1. Replace `elephc --web app.php` with `elephc --web-worker app.php`.
2. Move any per-request setup (reading the request, building a per-request
   object) into the registered handler closure.
3. Move any boot-once setup (DI container, config, connection pools) to the
   top level, before `elephc_worker_register`.
4. Audit `static` locals and static properties: in classic `--web` they reset
   per request, in worker mode they persist. If you relied on the reset, move
   the state into a per-request local inside the handler instead.
5. Set `--max-requests` to bound long-running worker memory growth (the worker
   default of 1000 recycles periodically).

See `examples/web-worker-hello/` for a runnable demo.

## Non-intrusive worker mode (`--web-worker=script`)

`--web-worker=script` is a third point on the axis between classic `--web` and
handler-mode `--web-worker`. Like `--web`, the **entire top level re-runs on
every request** and the superglobals are rebuilt fresh — but like handler mode,
**function `static` locals, static class properties, and global variables
persist across requests** within a worker process. There is **no API**: no
`elephc_worker_register`, no elephc-only syntax. The whole top level *is* the
per-request handler.

The payoff: you get persistent state (warm caches, long-lived connections)
**without changing the code**. The exact same file still runs unmodified under
php-fpm or `php -S` — persistence is simply expressed in portable PHP.

```bash
elephc --web-worker=script app.php
./app --listen 127.0.0.1:8080 --workers 4
```

### The three web modes

| | `--web` | `--web-worker` (handler) | `--web-worker=script` |
|---|---|---|---|
| Top-level | re-runs per request | boots once | re-runs per request |
| Function statics / static props | reset per request | persist | persist |
| Globals (`global`) | reset per request | persist | persist |
| Superglobals | fresh per request | fresh per request | fresh per request |
| Code changes | none | `elephc_worker_register` required | none |
| Runs under php-fpm | yes | no (elephc API) | yes |

### Boot-once in portable PHP

Because the top level re-runs per request but statics persist, the classic
null-guard idiom performs one-time setup that survives across requests:

```php
<?php
static $container = null;
if ($container === null) {
    $container = build_container(); // heavy: runs on request 1 only
}
$container->handle();               // runs on every request
```

On **request 1** the guard is `null`, so `build_container()` runs and its result
is stored in the `static`. On **request 2 and later** the `static` still holds
the container, the guard is non-null, and the build is skipped. Under classic
`--web` (or php-fpm) the `static` resets each request, so the same code rebuilds
every time — correct either way, just faster under script mode.

### What is fresh vs persistent

**Fresh on every request** (rebuilt by the runtime, same as classic `--web`):

- The seven superglobals: `$_SERVER`, `$_GET`, `$_POST`, `$_COOKIE`,
  `$_REQUEST`, `$_FILES`, `$_ENV`.
- `php://input` (the raw request body).
- The response status, headers, and body.

**Persistent for the worker's lifetime** (this is the whole point):

- **Function `static` locals** — retain their value across requests; their
  initializers run **once per worker**, not once per request. This differs from
  php-fpm, where a `static` initializer runs on every request.
- **Static class properties** — retain their value across requests; their
  initializers likewise run once per worker.
- **Global variables** — retain their value across requests.
- **Process-global state** — timezone (`date_default_timezone_set()`), current
  working directory (`chdir()`), and open file/directory/stream handles are
  **not reset** between requests. See the limitations below.

### Migration ladder

The modes form a zero-to-max-performance ladder:

1. **`--web`** — PHP-FPM semantics, full per-request isolation.
2. **`--web-worker=script`** — swap `--web` for `--web-worker=script`. **Zero
   code changes.** State that used to reset now persists; add a null-guard where
   you want boot-once behaviour. The same file still runs under php-fpm.
3. **`--web-worker`** (handler) — maximum performance: boot the app once, call
   `elephc_worker_register($handler)`, and the top level never re-runs. This
   step requires the elephc API, so the file no longer runs under php-fpm.

### Script-mode limitations

Script mode inherits classic `--web`'s [limitations](#limitations) and adds a
few that follow from persistence and the worker loop. The most important:

- **`exit()` / `die()` end the request, not the worker** (matching php-fpm).
  Calling `exit` or `die` — with or without parentheses (`exit;`, `die;`,
  `exit(0)`, `die("error")`), at any call depth — ends the current request: the
  output already `echo`-ed is flushed with the current status, code after the
  `exit` does not run, and the worker stays alive to serve the next request with
  its persistent statics intact. A string argument (`die("message")`) is written
  into the response body first. As in PHP, `exit` is a language construct, not
  an exception: it is **not catchable** by `catch (\Throwable)` and does **not**
  run `finally` blocks. The one exception is `--web-worker` handler mode, where
  the top level is a one-shot boot rather than a per-request entry — an `exit()`
  there still terminates the worker (and the master respawns it), so `return`
  from your handler instead.
- **`$argc` / `$argv` are not populated** — the binary's argv is consumed by the
  server (php-fpm does not set them either).
- **`ob_*` output buffering is not implemented.** The response
  body is still fully buffered by the runtime before it is sent, but the
  `ob_start()` family of builtins is unavailable.
- **Process-global state persists and is not reset.** Timezone changes
  (`date_default_timezone_set()`), a changed working directory (`chdir()`), and
  open file/directory/stream handles carry over from one request to the next.
  **Do not rely on per-request isolation of process state** — a request that
  changes the timezone or cwd affects every later request served by that worker.
  Set such process-global state once at the top of the top level (so it is
  re-applied per request) rather than assuming it starts clean.

## Concurrency model

The server uses a prefork model with `SO_REUSEPORT`: the master process forks N
worker processes before any request arrives, and the kernel load-balances
connections across workers.

Each worker is a separate process with its own copy of the runtime. Within a
single worker, requests are served **one at a time** — the PHP body runs to
completion before the next request is accepted. Parallelism equals the worker
count; a slow request occupies exactly one worker for its duration.

### Keep-alive and load distribution across workers

`SO_REUSEPORT` picks a worker for a connection **at accept time**, by hashing the
connection's 4-tuple (source/dest IP and port). HTTP/1.1 keep-alive then pins
**every** request on that connection to the same worker for the connection's
lifetime. Under a long-lived keep-alive load (e.g. `wrk -c1000`), the assignment
of connections to workers is frozen for the whole run: if one worker draws a slow
request, every connection pinned to it waits behind it, while other workers may be
idle. Unlike a shared work queue, elephc does not steal work between workers.

Two opt-in runtime flags can bound how long a connection stays pinned, so clients
reconnect periodically and the kernel re-picks a worker (a new source port
re-hashes the 4-tuple, landing on a statistically different worker):

- `--max-requests-per-connection N` (default `0`, off) — after N responses the
  server sends `Connection: close`; the client reconnects for its next request.
- `--idle-timeout SECS` (default `0`, off) — a connection idle for longer than SECS
  is closed, so a bursty client re-picks a worker on its next request.

Both default to `0` (disabled), which keeps connections pinned for their full
lifetime — the original, pre-rotation behavior — with no measurable overhead.
Set either to a positive value to opt in to rotation; smaller values (e.g.
`--max-requests-per-connection 16`) rebalance faster at the cost of more TCP
handshakes. Rotation is a cheap, opt-in mitigation, not a substitute for a
shared work queue (that is the separate fd-dispatch feature): within one
pinning window a slow request still blocks the requests queued behind it on
that worker, and in a mixed fast/slow benchmark aggressive rotation did not
improve — and could worsen — fast-route tail latency, which is why it ships
off by default. On Linux the `SO_REUSEPORT` hash spreads reconnects evenly; on
macOS the distribution is less uniform, so p99 validation benchmarks are
authoritative on Linux.

## Robustness

- **Graceful shutdown** — the master shuts down cleanly on `SIGINT` (Ctrl-C) and
  `SIGTERM`: it forwards termination to the workers, reaps them, and exits `0`. An
  in-flight request may be dropped when shutdown arrives.
- **Worker respawn** — a worker that dies unexpectedly (a crash, a
  `--max-execution-time` kill, or a handler-mode `exit()`) is replaced so the
  pool stays at `--workers` N. In `--web`/`--web-worker=script` mode `exit()`
  ends the request without killing the worker, so it no longer triggers a
  respawn.
- **Request body cap** — see `--max-body-size`; oversized bodies are rejected with
  `413` before the handler runs.
- **Idle and slow-connection bounds** — HTTP/1.1 keep-alive is enabled. When
  `--idle-timeout` is set (opt-in, default `0` = off), an idle keep-alive
  connection is closed after that many seconds by a per-connection watchdog;
  independently, hyper's `header_read_timeout` (30 s, anti-slowloris) always
  closes a connection whose next request's headers do not arrive in time. On
  the bundled hyper version the 30 s header timeout also caps the idle wait, so
  an `--idle-timeout` set above `30` is effectively capped at ~30 s; values `<=
  30` are honored by the watchdog, which fires first. Because a worker serves
  one connection at a time, a kept-alive connection holds a worker until it
  closes or times out — size `--workers` accordingly, or opt in to
  `--idle-timeout` / `--max-requests-per-connection` to rotate sooner.

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

`--web`, `--web-worker` (handler mode), and `--web-worker=script` (script mode)
each cannot be combined with `--check`, `--emit cdylib`, `--emit-asm`, or
`--emit-ir`, and the three web modes are mutually exclusive with each other (a
program is compiled in exactly one mode).
