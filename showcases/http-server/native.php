<?php
// native.php — POSIX socket layer for the elephc HTTP server showcase.
//
// All networking goes through libc via elephc's `extern` FFI. libc is linked
// by default on every supported target, so no -l flags are required. The
// socket-option and flag constants that differ between macOS and Linux are
// selected at runtime from PHP_OS.
//
// Every argument to an `extern` call is first materialized into a plain local
// variable. The compiler marshals plain variables into the C ABI reliably.

extern function socket(i32 $domain, i32 $type, i32 $protocol): i32;
extern function setsockopt(i32 $fd, i32 $level, i32 $optname, ptr $optval, u32 $optlen): i32;
extern function bind(i32 $fd, ptr $addr, u32 $addrlen): i32;
extern function listen(i32 $fd, i32 $backlog): i32;
extern function accept(i32 $fd, ptr $addr, ptr $addrlen): i32;
extern function poll(ptr $fds, u64 $nfds, i32 $timeout): i32;
extern function fcntl(i32 $fd, i32 $cmd, i32 $arg): i32;
extern function read(i32 $fd, ptr $buf, u64 $count): int;
extern function write(i32 $fd, string $buf, u64 $count): int;
extern function close(i32 $fd): i32;
extern function htons(u16 $hostshort): u16;
extern function malloc(u64 $size): ptr;
extern function free(ptr $p): void;
extern function memset(ptr $dest, i32 $byte, u64 $count): ptr;

// Socket constants gathered into one table. AF_INET, SOCK_STREAM and the
// fcntl commands are identical on every target; SOL_SOCKET, SO_REUSEADDR and
// O_NONBLOCK differ between macOS and Linux.
function sys_constants(): array {
    $c = [
        "AF_INET"     => 2,   // IPv4 address family
        "SOCK_STREAM" => 1,   // TCP — a reliable, ordered byte stream
        "F_GETFL"     => 3,   // fcntl: read descriptor flags
        "F_SETFL"     => 4,   // fcntl: write descriptor flags
    ];
    if (PHP_OS === "Darwin") {
        $c["SOL_SOCKET"]   = 65535;
        $c["SO_REUSEADDR"] = 4;
        $c["O_NONBLOCK"]   = 4;
    } else {
        $c["SOL_SOCKET"]   = 1;
        $c["SO_REUSEADDR"] = 2;
        $c["O_NONBLOCK"]   = 2048;
    }
    return $c;
}

// Create a listening TCP socket bound to $port on every interface.
// Returns the listener file descriptor, or -1 on failure.
function tcp_listen(int $port): int {
    $sys = sys_constants();
    $af = $sys["AF_INET"];
    $stream = $sys["SOCK_STREAM"];

    $fd = socket($af, $stream, 0);
    if ($fd < 0) {
        return -1;
    }

    // Let the port be re-bound immediately after the server restarts.
    $sol = $sys["SOL_SOCKET"];
    $reuse = $sys["SO_REUSEADDR"];
    $opt = malloc(4);
    ptr_write32($opt, 1);
    setsockopt($fd, $sol, $reuse, $opt, 4);
    free($opt);

    // Build a 16-byte `struct sockaddr_in`. The first two bytes differ by OS:
    // macOS stores [sin_len][sin_family], Linux stores a 16-bit sin_family.
    $addr = malloc(16);
    memset($addr, 0, 16);
    if (PHP_OS === "Darwin") {
        ptr_write8($addr, 16);                  // sin_len
        $family_cell = ptr_offset($addr, 1);
        ptr_write8($family_cell, $af);          // sin_family
    } else {
        ptr_write16($addr, $af);                // sin_family (16-bit)
    }
    $net_port = htons($port);
    $port_cell = ptr_offset($addr, 2);
    ptr_write16($port_cell, $net_port);         // sin_port, network byte order
    $ip_cell = ptr_offset($addr, 4);
    ptr_write32($ip_cell, 0);                   // sin_addr = INADDR_ANY

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

    // Make the listener non-blocking so a withdrawn connection cannot stall
    // accept() and freeze the whole event loop.
    $get_cmd = $sys["F_GETFL"];
    $set_cmd = $sys["F_SETFL"];
    $nonblock = $sys["O_NONBLOCK"];
    $flags = fcntl($fd, $get_cmd, 0);
    $new_flags = $flags | $nonblock;
    fcntl($fd, $set_cmd, $new_flags);
    return $fd;
}

// Accept one pending connection. Returns the client fd, or -1 when nothing is
// ready (the listener is non-blocking, so this never waits).
function socket_accept(int $listen_fd): int {
    $none = ptr_null();
    return accept($listen_fd, $none, $none);
}

// Read whatever bytes are currently available on $fd. Returns them as a
// string, or "" when the peer has closed the connection.
function socket_recv(int $fd): string {
    $cap = 65536;
    $buf = malloc($cap);
    if (ptr_is_null($buf)) {
        return "";
    }
    $n = read($fd, $buf, $cap);
    $data = "";
    if ($n > 0) {
        $data = ptr_read_string($buf, $n);
    }
    free($buf);
    return $data;
}

// Send a full response buffer to $fd. Returns the number of bytes written.
function socket_send(int $fd, string $data): int {
    $len = strlen($data);
    return write($fd, $data, $len);
}
