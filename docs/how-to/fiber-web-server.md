---
title: "Build a Fiber Web Server"
description: "How to build a native HTTP server with non-blocking sockets, poll(), and one Fiber per connection."
sidebar:
  order: 1
---

This guide explains the architecture behind the HTTP server showcase and how to build the same kind of server yourself. The complete implementation lives in [`showcases/http-server/`](../../showcases/http-server/); this page focuses on the pieces that make the design work.

The server uses:

- `extern function` declarations to call libc socket functions directly
- `ptr` memory helpers to build `sockaddr_in` and `pollfd` structs
- a non-blocking listening socket
- a single `poll()` event loop
- one `Fiber` per active connection

The result is cooperative concurrency: the process stays single-threaded, but each connection can pause when it needs more bytes and resume when the socket is readable again.

## File layout

Use small modules. The showcase splits the server into these files:

| File | Responsibility |
|---|---|
| `main.php` | Include the modules and call `run_http_server(8080)`. |
| `native.php` | Declare libc functions and wrap socket operations. |
| `http.php` | Parse HTTP requests and render HTTP responses. |
| `routes.php` | Dispatch parsed requests to application handlers. |
| `server.php` | Run the `poll()` loop and Fiber scheduler. |

That split keeps FFI, parsing, routing, and scheduling separate. It also keeps each function small, which is easier for the compiler and easier to debug.

## Declare the native socket API

Networking goes through libc with `extern function` declarations:

```php
<?php
extern function socket(int $domain, int $type, int $protocol): int;
extern function setsockopt(int $fd, int $level, int $optname, ptr $optval, int $optlen): int;
extern function bind(int $fd, ptr $addr, int $addrlen): int;
extern function listen(int $fd, int $backlog): int;
extern function accept(int $fd, ptr $addr, ptr $addrlen): int;
extern function poll(ptr $fds, int $nfds, int $timeout): int;
extern function fcntl(int $fd, int $cmd, int $arg): int;
extern function read(int $fd, ptr $buf, int $count): int;
extern function write(int $fd, string $buf, int $count): int;
extern function close(int $fd): int;
extern function htons(int $hostshort): int;
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;
extern function memset(ptr $dest, int $byte, int $count): ptr;
```

Materialize extern arguments into plain local variables before the call. This gives codegen simple C ABI values to marshal:

```php
<?php
$sys = sys_constants();
$af = $sys["AF_INET"];
$stream = $sys["SOCK_STREAM"];

$fd = socket($af, $stream, 0);
```

Some constants differ between macOS and Linux. Put them behind one helper and branch on `PHP_OS`:

```php
<?php
function sys_constants(): array {
    $c = [
        "AF_INET"     => 2,
        "SOCK_STREAM" => 1,
        "F_GETFL"     => 3,
        "F_SETFL"     => 4,
    ];

    if (PHP_OS === "Darwin") {
        $c["SOL_SOCKET"] = 65535;
        $c["SO_REUSEADDR"] = 4;
        $c["O_NONBLOCK"] = 4;
    } else {
        $c["SOL_SOCKET"] = 1;
        $c["SO_REUSEADDR"] = 2;
        $c["O_NONBLOCK"] = 2048;
    }

    return $c;
}
```

## Create a non-blocking listener

The listener is a normal TCP socket bound with `socket()`, `setsockopt()`, `bind()`, and `listen()`. The low-level part is building the 16-byte `struct sockaddr_in` with pointer writes:

```php
<?php
function tcp_listen(int $port): int {
    $sys = sys_constants();
    $af = $sys["AF_INET"];
    $stream = $sys["SOCK_STREAM"];

    $fd = socket($af, $stream, 0);
    if ($fd < 0) {
        return -1;
    }

    $addr = malloc(16);
    memset($addr, 0, 16);

    if (PHP_OS === "Darwin") {
        ptr_write8($addr, 16);
        $family_cell = ptr_offset($addr, 1);
        ptr_write8($family_cell, $af);
    } else {
        ptr_write16($addr, $af);
    }

    $net_port = htons($port);
    $port_cell = ptr_offset($addr, 2);
    ptr_write16($port_cell, $net_port);

    $ip_cell = ptr_offset($addr, 4);
    ptr_write32($ip_cell, 0);

    $bound = bind($fd, $addr, 16);
    free($addr);
    if ($bound != 0) {
        close($fd);
        return -1;
    }

    if (listen($fd, 128) != 0) {
        close($fd);
        return -1;
    }

    $get_cmd = $sys["F_GETFL"];
    $set_cmd = $sys["F_SETFL"];
    $nonblock = $sys["O_NONBLOCK"];
    $flags = fcntl($fd, $get_cmd, 0);
    fcntl($fd, $set_cmd, $flags | $nonblock);

    return $fd;
}
```

The listener must be non-blocking so a withdrawn connection cannot freeze `accept()`. Client reads are only performed after `poll()` reports the socket readable; if you add any operation that can wait without going through `poll()`, one connection can stall the whole process.

## Store connection state

Each connection needs a file descriptor, an input buffer, lifecycle flags, and its Fiber:

```php
<?php
class Connection {
    public int $fd = 0;
    public $fiber = null;
    public string $inbuf = "";
    public bool $active = false;
    public bool $started = false;
    public bool $closed = false;

    public function reset(int $fd): void {
        $this->fd = $fd;
        $this->fiber = null;
        $this->inbuf = "";
        $this->active = true;
        $this->started = false;
        $this->closed = false;
    }
}
```

The showcase uses a fixed pool of `Connection` objects. Idle slots are reused, so steady-state traffic does not allocate a new PHP object for every request.

## Put each connection in a Fiber

The Fiber owns the request flow for one client: read bytes, wait for a complete header, parse, route, respond, and close. When the request is incomplete, it calls `Fiber::suspend()` and gives control back to the event loop:

```php
<?php
function make_conn_fiber(Connection $conn) {
    return new Fiber(function() use ($conn): void {
        $fd = $conn->fd;

        while (strpos($conn->inbuf, "\r\n\r\n") === false) {
            $chunk = socket_recv($fd);
            if ($chunk === "") {
                close($fd);
                $conn->closed = true;
                return;
            }

            $conn->inbuf = $conn->inbuf . $chunk;

            if (strpos($conn->inbuf, "\r\n\r\n") === false) {
                Fiber::suspend(0);
            }
        }

        $req = parse_request($conn->inbuf);
        $res = handle_request($req);
        socket_send($fd, $res->render());

        close($fd);
        $conn->closed = true;
    });
}
```

The event loop starts the Fiber the first time the connection is readable, then resumes it on later readiness notifications:

```php
<?php
function service_connection(Connection $conn): void {
    if (!$conn->started) {
        $conn->started = true;
        $conn->fiber = make_conn_fiber($conn);
        $conn->fiber->start();
    } elseif (!$conn->fiber->isTerminated()) {
        $conn->fiber->resume(0);
    }
}
```

Nothing runs in parallel. The server is asynchronous because each Fiber yields at explicit I/O wait points.

## Drive everything with poll()

The event loop stores one `struct pollfd` for the listener and one for each active connection. A `pollfd` is 8 bytes on the supported targets:

| Offset | Field |
|---|---|
| `0` | `fd` as a 32-bit integer |
| `4` | `events` as a 16-bit integer |
| `6` | `revents` as a 16-bit integer |

Small helpers make the pointer writes readable:

```php
<?php
function poke32(ptr $p, int $off, int $value): void {
    $cell = ptr_offset($p, $off);
    ptr_write32($cell, $value);
}

function poke16(ptr $p, int $off, int $value): void {
    $cell = ptr_offset($p, $off);
    ptr_write16($cell, $value);
}

function peek16(ptr $p, int $off): int {
    $cell = ptr_offset($p, $off);
    return ptr_read16($cell);
}
```

The loop has four phases:

1. Rebuild the `pollfd` array from the listener and active connections.
2. Call `poll($pollfds, $nfds, -1)` to wait for activity.
3. Resume every connection whose `revents` field is non-zero.
4. Accept a new client if the listener is ready, then recycle closed slots.

In outline:

```php
<?php
function run_http_server(int $port): void {
    $max = 64;
    $listen_fd = tcp_listen($port);
    $slots = [];

    $i = 0;
    while ($i < $max) {
        $slots[] = new Connection();
        $i = $i + 1;
    }

    $pollfds = malloc(($max + 1) * 8);

    while (true) {
        poke32($pollfds, 0, $listen_fd);
        poke16($pollfds, 4, 1);
        poke16($pollfds, 6, 0);

        $poll_map = [];
        $active = 0;

        $i = 0;
        while ($i < $max) {
            if ($slots[$i]->active) {
                $base = ($active + 1) * 8;
                poke32($pollfds, $base, $slots[$i]->fd);
                poke16($pollfds, $base + 4, 1);
                poke16($pollfds, $base + 6, 0);
                $poll_map[] = $i;
                $active = $active + 1;
            }
            $i = $i + 1;
        }

        $nfds = $active + 1;
        if (poll($pollfds, $nfds, -1) < 0) {
            break;
        }

        $p = 0;
        while ($p < $active) {
            $rev_off = ($p + 1) * 8 + 6;
            if (peek16($pollfds, $rev_off) != 0) {
                service_connection($slots[$poll_map[$p]]);
            }
            $p = $p + 1;
        }

        if (peek16($pollfds, 6) != 0 && $active < $max) {
            $cfd = socket_accept($listen_fd);
            if ($cfd >= 0) {
                // Find a free slot and call $slots[$free]->reset($cfd).
            }
        }

        // Mark closed slots inactive so they can be reused.
    }

    close($listen_fd);
}
```

The actual showcase fills in the free-slot search, socket receive/send wrappers, parser, router, and error handling.

## Parse and route requests

For a minimal HTTP/1.1 server, wait until `\r\n\r\n` appears in the input buffer, then split the request into:

- request line: `METHOD /path?query HTTP/1.1`
- headers
- optional body

The showcase models this with a `Request` class, a `Response` class, and a plain dispatcher:

```php
<?php
function handle_request(Request $req): Response {
    if ($req->method !== "GET") {
        $res = new Response();
        $res->status = 405;
        $res->text("405 Method Not Allowed\n");
        return $res;
    }

    if ($req->path === "/") {
        return route_index();
    } elseif ($req->path === "/hello") {
        return route_hello($req);
    }

    return route_not_found($req);
}
```

Prefer named handler functions over a runtime table of handler closures. The showcase uses explicit dispatch because it is predictable for ahead-of-time codegen and still easy to read.

## Build and test it

Compile the complete showcase:

```bash
cargo run -- showcases/http-server/main.php
./showcases/http-server/main
```

Then exercise routes from another terminal:

```bash
curl http://127.0.0.1:8080/
curl 'http://127.0.0.1:8080/hello?name=elephc'
curl http://127.0.0.1:8080/json
curl http://127.0.0.1:8080/stats
```

The showcase also includes a helper:

```bash
./showcases/http-server/build.sh run
./showcases/http-server/build.sh test
```

## Design notes

- Fibers are cooperative. A connection only yields when the code calls `Fiber::suspend()`.
- Put every potentially blocking socket operation behind `poll()`. A blocking wait inside one Fiber would still block the whole process.
- `poll()` is the scheduler. It decides which connection Fiber should be resumed next.
- Use `Connection: close` first. Keep-alive, request bodies, TLS, and chunked transfer encoding add real protocol complexity.
- Keep native structs isolated behind small pointer helpers. Most application code should work with PHP objects and strings.

For the language details behind this design, see [Fibers](../php/fibers.md), [FFI & Extern](../beyond-php/extern.md), and [Pointers](../beyond-php/pointers.md).
