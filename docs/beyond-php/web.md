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

## Request model

The request model follows PHP-FPM / `php -S`: each incoming HTTP request
re-runs the program's top-level code from a completely fresh state. Whatever
the script writes via `echo` or `print` becomes the HTTP response body, returned
as `200 OK` with no `Content-Type` or other custom headers set. Custom status
codes and response headers (`header()`, `http_response_code()`) arrive in Phase 3.

```php
<?php
echo "Hello from elephc-web!";
```

Compiled with `--web`, the binary above serves `Hello from elephc-web!` for
every request.

See `examples/web-hello/` for a minimal runnable demo.

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

## Phase 1 limitations

Phase 1 delivers the core serve loop (echo → body, fresh per-request state,
prefork/SO_REUSEPORT). Several features are not yet available and will arrive in
later phases:

- **No request input** — `$_SERVER`, `$_GET`, `$_POST`, `php://input`, and all
  other request superglobals are not populated in Phase 1. Phase 2 will add
  request input.
- **No response control** — `header()` and `http_response_code()` are not
  available. Every Phase 1 response is `200 OK`; custom status codes and headers
  arrive in Phase 3.
- **`$argc` / `$argv` not populated** — the binary's own argv is consumed by
  the server and is not forwarded to the script body.
- **No intra-worker concurrency** — one slow request occupies its worker until
  it completes. Use `--workers` to increase throughput.
- **Not supported in this release (all phases):** multipart/form-data, file
  uploads, cookies, sessions, static file serving, in-process TLS, HTTP/2–3.

## Mutual exclusions

`--web` cannot be combined with `--check`, `--emit cdylib`, `--emit-asm`, or
`--emit-ir`.
