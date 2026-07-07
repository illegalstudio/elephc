---
title: "CLI reference"
description: "The complete, authoritative list of every elephc command-line flag, its accepted values, default, and environment-variable override."
sidebar:
  order: 3
---

This page lists every flag the `elephc` command accepts. Topical pages
([optimization](optimization.md), [output](output-and-diagnostics.md),
[linking](linking-and-conditional-compilation.md)) explain the *why*; this page is
the exhaustive *what*.

## Synopsis

```text
elephc [OPTIONS] <source.php>
```

Exactly one positional argument is required: the path to the PHP source file. The
binary is written next to it, named after the source without its extension.

## Input and output

| Flag | Values | Default | Description |
|---|---|---|---|
| `<source.php>` | path | — | Required. The PHP file to compile. |
| `--emit KIND` / `--emit=KIND` | `executable` (`exe`, `bin`), `cdylib` (`dylib`, `shared`) | `executable` | Output artifact kind. `cdylib` builds a C-ABI shared library. |
| `--emit-asm` | — | off | Write generated assembly instead of a binary. |
| `--emit-ir` | — | off | Print the EIR textual form and stop. |
| `--check` | — | off | Run front-end checks only; write nothing. |
| `--source-map` | — | off | Emit a `.map` JSON sidecar next to the assembly ([schema](source-maps.md)). |
| `--debug-info` | — | off | Embed DWARF `.file`/`.loc` line directives in the assembly for lldb/gdb/profilers. |
| `--web` | — | off | Compile a prefork HTTP server binary instead of a CLI executable. See [Web Server](../beyond-php/web.md). |
| `--web-worker` / `--web-worker=handler` | — | off | Compile a worker-**handler**-mode HTTP server binary: the top-level boots once and registers a per-request handler (via the `elephc_worker_register` API) instead of re-running the program per request. See [Web Server — Worker mode](../beyond-php/web.md#worker-mode---web-worker). |
| `--web-worker=script` | — | off | Compile a worker-**script**-mode HTTP server binary: no registration API required, the top-level program re-runs per request, but statics, class properties, and globals persist across requests within the same worker process. The source program uses no elephc-specific API, so the same `.php` file also runs unmodified under php-fpm or `php -S`. |

`--emit-ir`, `--emit-asm`, and `--check` are mutually exclusive. `--web`,
`--web-worker` (handler mode), and `--web-worker=script` (script mode) are
mutually exclusive with each other and each cannot be combined with `--check`,
`--emit cdylib`, `--emit-asm`, or `--emit-ir`. See
[Output formats and diagnostics](output-and-diagnostics.md).

## Web server binary runtime arguments

When a program is compiled with `--web`, `--web-worker` (handler mode), or
`--web-worker=script` (script mode), the produced binary accepts these runtime
arguments (not elephc compiler flags):

| Argument | Required | Default | Description |
|---|---|---|---|
| `--listen host:port` | Yes | — | Address and port to bind. Missing `--listen` prints an error to stderr and exits non-zero. |
| `--workers N` | No | CPU count | Number of prefork worker processes. Minimum 1. |
| `--dispatch MODE` | No | `kernel` | Connection dispatch: `kernel` = per-worker `SO_REUSEPORT` listeners (kernel picks the worker); `master` = the master accepts and hands each connection to an idle worker (shared queue, no head-of-line blocking between workers). Unknown value = usage error (exit 2). See [Connection dispatch](../beyond-php/web.md#connection-dispatch-kernel-vs-master). |
| `--dispatch-backlog N` | No | `1024` | Master mode only: max accepted connections queued while all workers are busy before the master applies SYN backpressure. Ignored (warns) without `--dispatch master`. |
| `--handler-offload` | No | off | Run the PHP handler on a dedicated `php-handler` thread fed by a bounded job queue, so request/response I/O of other connections overlaps handler execution (handlers still never overlap — one consumer thread). Opt-in; default off keeps the synchronous inline handler path unchanged. Same in all three web modes. See [Handler offload](../beyond-php/web.md#handler-offload). |
| `--max-pending N` | No | `16` | With `--handler-offload`: max parsed requests queued for the handler thread before new requests get `503 Service Unavailable` + `Retry-After: 1` (built on the I/O thread, no PHP). Bounds queued-body memory to `N × --max-body-size`; `0` is rejected (exit 2). Ignored (warns) without `--handler-offload`. |
| `--max-body-size N` | No | `8388608` (8 MiB) | Max request body in bytes (`0` = unlimited); oversized bodies get `413`. |
| `--max-requests N` | No | `0` (classic) / `1000` (worker) | Recycle each worker process after N requests (bounds memory growth). Worker mode defaults to 1000. |
| `--max-rss MiB` | No | `0` (off) | Recycle a worker whose resident set exceeds this many MiB; `0` = off (the default). Measurement is gated to at most once per 64 accepts. Same in all three web modes. |
| `--reload-grace SECS` | No | `10` | Max seconds a worker waits for in-flight requests to finish during a SIGHUP rolling reload before the master force-recycles it; `0` = wait forever. Drain always happens on SIGUSR1; the flag only bounds the wait. Same in all three web modes. See [SIGHUP rolling reload](../beyond-php/web.md#sighup-rolling-reload). |
| `--max-requests-per-connection N` | No | `0` (opt-in) | Close a keep-alive connection after N responses (sends `Connection: close`) so the client reconnects and the kernel re-picks a worker; `0` = unlimited (off by default; no behavior change unless set). Same default in all three web modes. |
| `--idle-timeout SECS` | No | `0` (opt-in) | Close a keep-alive connection idle (no new request) for more than SECS seconds; `0` = never (off by default; no behavior change unless set). Same default in all three web modes. |
| `--worker-gc-interval N` | No | `0` (classic) / `1` (worker) | Run the cycle collector every N requests (`0` = never, `1` = every request). Worker-mode only. |
| `--max-execution-time N` | No | `0` (none) | Kill a handler that runs longer than N seconds; the master respawns the worker. |
| `--gzip` | No | off | Gzip-compress responses when the client sends `Accept-Encoding: gzip`. |
| `--access-log` | No | off | Log one line per request to stderr. |
| `--tls-cert FILE` | No | — | PEM certificate chain; enables TLS on `--listen`. Requires `--tls-key`. See [TLS / HTTPS](../beyond-php/web.md#tls--https). |
| `--tls-key FILE` | No | — | PEM private key (PKCS#8/PKCS#1/SEC1, unencrypted) matching `--tls-cert`. Requires `--tls-cert`. |
| `--http2` | No | off | Opt in to HTTP/2 (h2c prior-knowledge on plaintext; h2 over TLS is a follow-up). **Requires `--handler-offload`** (without offload, multiplexed h2 streams all stall on the single inline handler; exit 2 otherwise). Default off: the server speaks HTTP/1.1 only via one `http1_only()` code path. See [HTTP/2](../beyond-php/web.md#http2). |
| `--http2-max-streams N` | No | `8` | Max concurrent h2 streams per connection. Also the per-connection stream budget (with `--max-requests-per-connection` as the cap, else `--max-requests`). Per-connection memory bound is `N × --max-body-size`. `N < 1` is rejected (exit 2). Ignored (warns) without `--http2`. |
| `--http2-max-header-size N` | No | `65536` (64 KiB) | HPACK header-bomb clamp in bytes (hyper `max_header_list_size`). h1 is unaffected. Ignored (warns) without `--http2`. |
| `--help`, `--version` | No | — | Print usage / version and exit. |

```bash
elephc --web app.php
elephc --web-worker app.php
elephc --web-worker=script app.php
./app --listen 127.0.0.1:8080
./app --listen 0.0.0.0:8080 --workers 4 --max-body-size 1048576 --access-log
./app --listen 0.0.0.0:8080 --workers 8 --dispatch master --max-requests-per-connection 32
./app --listen 127.0.0.1:8443 --tls-cert cert.pem --tls-key key.pem
```

The served program also receives `$_COOKIE`, `$_REQUEST`, and `$_ENV`, and can
emit cookies with `setcookie()`. The server shuts down cleanly on
`SIGINT`/`SIGTERM` and respawns workers that die.

The served program receives the HTTP request through the standard superglobals
`$_SERVER`, `$_GET`, `$_POST`, and `php://input`, and controls the response
status and headers with `http_response_code()` and `header()`. See
[Web Server](../beyond-php/web.md#request-input).

## Targets

| Flag | Values | Default | Description |
|---|---|---|---|
| `--target TARGET` / `--target=TARGET` | `macos-aarch64`, `linux-aarch64`, `linux-x86_64` (plus alias spellings) | host platform | Select the compilation target. |

See [Targets and cross-compilation](targets.md) for the full list of accepted
spellings.

## Optimization and code generation

| Flag | Values | Default | Env override | Description |
|---|---|---|---|---|
| `--ir-opt=on\|off` | `on`, `off` | `on` | `ELEPHC_IR_OPT` | Toggle the EIR optimization passes: identity folding, peepholes, constant folding, common-subexpression elimination, loop-invariant code motion, dead-instruction elimination, dead-store elimination, branch simplification, and the cross-function small-function inliner — run to a module-level fixed point. |
| `--no-ir-opt` | — | — | `ELEPHC_IR_OPT=off` | Shorthand for `--ir-opt=off`. |
| `--regalloc=linear\|stack` | `linear`, `stack` | `linear` | `ELEPHC_REGALLOC` | Register allocator: linear-scan, or stack-only fallback. |
| `--null-repr=sentinel\|tagged` | `sentinel`, `tagged` | `tagged` | `ELEPHC_NULL_REPR` | Representation for null-capable scalar slots. |

See [Optimization and codegen controls](optimization.md).

## Linking and FFI

| Flag | Values | Default | Description |
|---|---|---|---|
| `--link LIB` / `-l LIB` / `-lLIB` | library name | — | Link an extra native library (repeatable). |
| `--link-path DIR` / `-L DIR` / `-LDIR` | directory | — | Add a library search path (repeatable). |
| `--framework NAME` | framework name | — | Link a macOS framework (repeatable). |
| `--with-CRATE` | `pdo`, `tls`, `crypto`, `phar`, `tz`, `image`, `web` | — | Force-enable a bridge crate regardless of feature auto-detection (repeatable). Force-links the staticlib (whole-archived, so it is not dead-stripped) and, for crates with a PHP-surface prelude (`pdo`, `tz`, `image`), force-injects that prelude so the API is available. `--with-web` is an alias for `--web`. An unknown crate name is an error. |

See [Linking, heap, and conditional compilation](linking-and-conditional-compilation.md).

## Memory and conditional compilation

| Flag | Values | Default | Description |
|---|---|---|---|
| `--heap-size=BYTES` | integer ≥ 65536 | `8388608` (8 MB) | Size of the program's runtime heap. |
| `--define SYMBOL` / `--define=SYMBOL` | symbol name | — | Define a compile-time symbol for `ifdef` (repeatable). |

## Diagnostics and debugging

| Flag | Values | Default | Description |
|---|---|---|---|
| `--timings` | — | off | Print per-phase compiler timings to stderr. |
| `--gc-stats` | — | off | Print allocation/free counters at exit. |
| `--heap-debug` | — | off | Enable runtime heap verification (double-free, bad refcount, free-list corruption). |

See [Output formats and diagnostics](output-and-diagnostics.md).

## Environment variables

Three environment variables provide defaults that the matching flag overrides.
They exist mainly so a whole test run or benchmark can flip a default without
changing every invocation:

| Variable | Values | Equivalent flag |
|---|---|---|
| `ELEPHC_IR_OPT` | `on`, `off` | `--ir-opt=` |
| `ELEPHC_REGALLOC` | `linear`, `stack` | `--regalloc=` |
| `ELEPHC_NULL_REPR` | `tagged`, `sentinel` | `--null-repr=` |
