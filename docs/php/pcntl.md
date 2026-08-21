---
title: "PCNTL"
description: "Forking, waiting, process replacement, signals, affinity, namespaces, and target-specific process controls."
sidebar:
  order: 23
---

elephc implements PHP 8.4's PCNTL process-control surface on macOS AArch64,
Linux AArch64, and Linux x86_64. PCNTL calls are backed by the optional
`elephc-pcntl` bridge and produce standalone binaries; the target machine does
not need PHP or the PHP PCNTL extension.

Using a PCNTL function auto-links the bridge. Use `--with-pcntl` when calls are
only discoverable at runtime, for example through opaque dynamic `eval()`:

```bash
elephc --with-pcntl worker.php
```

`extension_loaded('pcntl')` reports `true` whenever the bridge is linked.

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

`pcntl_waitid()` writes the target-supported siginfo fields on success. The
usual status helpers are available: `pcntl_wifexited`, `pcntl_wexitstatus`,
`pcntl_wifsignaled`, `pcntl_wtermsig`, `pcntl_wifstopped`, `pcntl_wstopsig`, and
`pcntl_wifcontinued` where the host libc supports it.

`pcntl_exec()` replaces the current process and never returns on success. Its
argument and environment arrays are copied into native `argv`/`envp` storage;
an omitted environment inherits the current process environment, while an
explicit empty array clears it. OS failures emit PHP's warning, return `false`,
and remain available through `pcntl_get_last_error()` / `pcntl_errno()`.

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
- starting, resuming, or suspending a Fiber from a handler throws `FiberError`
  because switching execution contexts during dispatch is unsafe.

`pcntl_signal_get_handler()` returns the registered callable or integer
disposition. An invalid signal number throws `ValueError`. If the OS rejects a
valid registration, `pcntl_signal()` emits `E_WARNING` and returns `false`.

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
dispositions, queue, dispatch masking, wait outputs, and warning behavior as
AOT code. Callable descriptors remain owned by the active backend runtime:
Magician retains eval handlers in its evaluation context, while compiled
handlers remain in AOT runtime storage.

Because those handler tables use different callable representations, signal
registration and dispatch must currently stay in the same backend domain. An
eval handler must be dispatched from eval, and a compiled handler must be
dispatched from compiled code. If the other backend drains the shared queue
first, it has no compatible descriptor to invoke for that record. This is the
only intentional callable-interoperability limit; ordinary process, wait,
mask, errno, and disposition state remains shared.

The pending-signal transport is a nonblocking process-local pipe. This keeps
the OS handler async-signal-safe, but the queue is bounded by the operating
system's pipe capacity; a sustained burst that fills it can lose later records.
As in PHP, applications should keep handlers short and move work into their
normal event loop. PCNTL is unavailable on Windows because Windows is not in
elephc's supported target matrix.

See the generated [`pcntl_fork()` reference](./builtins/misc/pcntl_fork.md) and
the neighboring PCNTL builtin pages for individual signatures and backend
support.
