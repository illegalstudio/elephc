---
title: "PCNTL"
description: "Forking, waiting, process replacement, signals, sessions, daemonization, affinity, namespaces, and target-specific controls."
sidebar:
  order: 23
---

elephc implements the maintained PCNTL process-control surface, including PHP
8.5's `pcntl_waitid()` resource-usage output, on macOS AArch64, Linux AArch64,
and Linux x86_64. PCNTL calls are backed by the optional
`elephc-pcntl` bridge and produce standalone binaries; the target machine does
not need PHP or the PHP PCNTL extension.

Using a PCNTL function auto-links the bridge. Use `--with-pcntl` when calls are
only discoverable at runtime, for example through opaque dynamic `eval()`:

```bash
elephc --with-pcntl worker.php
```

`extension_loaded('pcntl')` reports `true` whenever the bridge is linked.

PCNTL is intentionally unavailable for both `ios-arm64` and `ios-sim-arm64`, and
for `--emit cdylib` or `--emit staticlib`. A hosted library shares its embedding
process, so forking, replacing the process image, or changing process-wide
signal state would escape the recoverable export boundary and affect the host
application.

## Fork, wait, and exec

`pcntl_fork()` returns the child PID to the parent and `0` to the child. The
child inherits normal PHP state through the operating system's copy-on-write
fork semantics, but it receives a private pending-signal queue: signals sent to
the child cannot be dispatched by the parent, and vice versa.

```php
$pid = pcntl_fork();

if ($pid === -1) {
    echo pcntl_strerror(pcntl_get_last_error());
    exit(1);
}

if ($pid === 0) {
    echo "child\n";
    exit(23);
}

$waited = pcntl_waitpid($pid, $status, 0, $usage);
if ($waited === $pid && pcntl_wifexited($status)) {
    echo pcntl_wexitstatus($status); // 23
}
```

`pcntl_wait()` and `pcntl_waitpid()` populate `$resource_usage` with PHP's 17
`getrusage` fields only when a child PID greater than zero is returned. A failed
wait preserves the caller's prior `$status` and replaces `$resource_usage` with
an empty array. A `WNOHANG` result of `0` also leaves usage empty.

`pcntl_waitid()` writes the target-supported siginfo fields on success. Its PHP
8.5 fifth `&$resource_usage` argument is populated with the same 17 fields on
Linux, including a successful `WNOHANG` result. A Linux syscall failure writes
an empty usage array while preserving the prior siginfo output. macOS accepts
the portable signature but leaves the usage variable untouched, matching
php-src's non-Linux path where neither the raw Linux syscall nor `wait6()` is
available. The usual status helpers are available: `pcntl_wifexited`,
`pcntl_wexitstatus`, `pcntl_wifsignaled`, `pcntl_wtermsig`, `pcntl_wifstopped`,
`pcntl_wstopsig`, and `pcntl_wifcontinued` where the host libc supports it.

`pcntl_exec()` replaces the current process and never returns on success. Its
argument and environment arrays are copied into native `argv`/`envp` storage;
an omitted environment inherits the current process environment, while an
explicit empty array clears it. Embedded null bytes raise PHP's position-specific
`ValueError` before entering the OS. Other OS failures emit PHP's warning,
return `false`, and remain available through `pcntl_get_last_error()` /
`pcntl_errno()`.

## Process groups, sessions, and daemonization

`posix_setpgid($process_id, $process_group_id)` moves a process into a process
group and returns a boolean result. `posix_setsid()` creates a new session for
the calling process and returns its session identifier, or `-1` after an OS
failure. Both operations preserve the native errno for
`pcntl_get_last_error()`.

Elephc also provides `pcntl_daemon(bool $no_chdir = false, bool $no_close =
false): bool` as a convenience wrapper over the host's `daemon(3)`. This is an
Elephc extension rather than a PHP 8.4 PCNTL function, so `--strict-php` hides
it. The surviving daemon process receives private pending-signal queues just as
a child created by `pcntl_fork()` does.

## Signal handlers and dispatch

Register `SIG_DFL`, `SIG_IGN`, or any supported PHP callable with
`pcntl_signal()`. The default `$restart_syscalls` value follows PHP: it is
`false` for `SIGALRM` and `true` for other signals when the third argument is
omitted.

```php
$seen = [];

pcntl_signal(SIGUSR1, function (int $signal, array $info) use (&$seen): void {
    $seen[] = [$signal, $info['signo']];
});

pcntl_signal_dispatch();
```

The native OS handler only enqueues a stable record. PHP callables run later at
`pcntl_signal_dispatch()` or at an async safe point after
`pcntl_async_signals(true)`. Dispatch has these PHP-compatible guarantees:

- it is non-reentrant;
- it processes one snapshot, so signals arriving during a handler wait for the
  next dispatch;
- signals stay masked while the snapshot is processed;
- an exception stops the current snapshot, discards its remaining records,
  restores the signal mask and dispatch state, then propagates normally;
- starting, resuming, throwing into, or suspending a Fiber from a handler throws
  `FiberError` because switching execution contexts during dispatch is unsafe.

`pcntl_signal_get_handler()` returns the registered callable in its original PHP
shape: named handlers remain strings, method handlers remain arrays, and closures
remain `Closure` objects. `SIG_DFL`, `SIG_IGN`, and never-configured signals return
their integer dispositions. A later eval context may fetch and invoke an eval
handler, or wrap it with `Closure::fromCallable()`, while the owning context
remains pinned. That foreign descriptor cannot cross from eval into compiled
storage: direct results and nested array/object/global values that contain it are
refused. An invalid signal number throws `ValueError`. If the OS rejects a valid
registration, including attempts to handle `SIGKILL` or `SIGSTOP`,
`pcntl_signal()` raises PHP's unsuppressible fatal error and terminates the
process.

An eval context detached by a registered handler can therefore remain alive for
the rest of the process. It is reclaimed only after its last handler is
replaced by another callable, `SIG_DFL`, or `SIG_IGN`; it is never freed while a
native signal trampoline still names it. A foreign eval handler may be fetched,
wrapped, and invoked inside a later eval context. Returning or assigning that
context-local descriptor across the eval-to-AOT boundary is refused before the
owner can be freed, including when an array, object, or `$GLOBALS` entry hides
the descriptor.

Signal masks use `pcntl_sigprocmask()`. Linux additionally provides
`pcntl_sigwaitinfo()` and `pcntl_sigtimedwait()` for synchronous signal receipt.
Invalid dynamic mask modes, empty signal sets, out-of-range signals, and invalid
timed-wait durations raise the same `ValueError` cases as PHP; they are not
collapsed into a silent `false` result.

## Target-specific surface

| Target | Additional functions |
|---|---|
| Linux AArch64 / x86_64 | `pcntl_getcpu`, `pcntl_getcpuaffinity`, `pcntl_setcpuaffinity`, `pcntl_setns`, `pcntl_sigwaitinfo`, `pcntl_sigtimedwait`, `pcntl_unshare` |
| macOS AArch64 | `pcntl_getqos_class`, `pcntl_setqos_class`, and `Pcntl\QosClass` |

Function availability and PCNTL constants are selected from the compilation
target, not from the machine running the compiler. Linux-only functions are
undefined in macOS output, and the macOS QoS API is undefined in Linux output.
Linux namespace and CPU-affinity argument failures are classified separately
from OS permission/resource failures: invalid values raise `ValueError`, while
operating-system failures emit a suppressible PHP warning and return `false`.
Invalid priority selector modes likewise raise target-specific `ValueError`s on
all supported targets.
`pcntl_rfork()` and `pcntl_forkx()` belong to operating systems outside
elephc's supported target matrix and are intentionally absent.

## Eval and current limits

Dynamic `eval()` uses the same native bridge, errno state, process signal
dispositions, dispatch masking, wait outputs, and warning behavior as AOT code.
Callable descriptors remain owned by the backend that registered them:
Magician retains eval handlers in a process-global registry that keeps their
owning eval context alive across generated function-frame teardown, while
compiled handlers remain in AOT runtime storage. Pending records are routed
into separate AOT and eval queues, so dispatching from the other backend cannot
consume or drop them. An eval handler is therefore invoked by an eval dispatch,
and a compiled handler by a compiled dispatch; registrations cannot be
inspected as the other backend's incompatible callable representation.

The native `sigaction` disposition is still process-wide. If AOT and Magician
both install a handler for the same signal, the later installer owns future OS
delivery; records queued before that replacement stay with their original
backend. This last-installer-wins rule does not merge the callable tables or
make their descriptors interchangeable.

Each pending-signal queue uses a nonblocking process-local pipe so the OS
handler remains async-signal-safe. If the pipe fills, later deliveries spill
into a preallocated 4096-record lock-free queue and are replayed in FIFO order
after pipe records; every retained delivery keeps its own siginfo snapshot. If
both the pipe and those 4096 spill slots are full, newer deliveries are dropped.
PHP 8.5 has the same bounded-overload rule: it preallocates `num_signals`
pending records and drops a delivery when that pool is exhausted. Independently,
the operating system may coalesce repeated standard non-realtime signals while
one is already pending, before either PHP or elephc's handler runs; realtime
signals retain the OS's queued-delivery semantics.
As in PHP, applications should keep handlers short and move work into their
normal event loop. PCNTL is unavailable on Windows because Windows is not in
elephc's supported target matrix.

See the generated [`pcntl_fork()` reference](./builtins/misc/pcntl_fork.md) and
the neighboring PCNTL builtin pages for individual signatures and backend
support.
