---
title: "Structured Parallel Tasks"
description: "Run transferable PHP closures in isolated native worker contexts."
sidebar:
  order: 11
---

`Elephc\Parallel` executes PHP closures on bounded native worker threads. Every worker owns a
separate runtime context, heap arena, stack, exception chain, serializer state, handles, resources,
and output buffers. The operating system schedules admitted threads; Elephc provides FIFO admission
and structured ownership rather than a second CPU scheduler.
The Parallel PHP declarations are injected when compilation sees `Elephc\Parallel\run()`,
including function/namespace aliases and calls relative to the current namespace.
`--with-parallel` only force-links the worker bridge and does not itself declare the API.

## Run a structured scope

```php
<?php

use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

$result = run(static function (TaskGroup $tasks): array {
    $left = $tasks->spawn(static fn (int $value): int => $value * 2, 21);
    $right = $tasks->spawn(static fn (string $name): string => "hello " . $name, "elephc");

    return [$left->join(), $right->join()];
});

echo $result[0], "|", $result[1];
```

`run()` does not return until every submitted child is terminal and its native thread has been
joined. `Future::join()` explicitly blocks the calling OS thread. A Future returned from its owning
scope retains only copied terminal wire bytes (never the worker context or arena), so it remains
usable after `run()` returns; repeated successful joins decode the same terminal result, while a
failed escaped Future retains and rethrows its parent-side `TaskFailure` even after a caught scope
aggregate. `Future::isComplete()` is non-blocking and returns `true` for every terminal native
phase: successful, failed, or cancelled. It does not observe or consume a failure. Future
construction and TaskGroup bookkeeping hooks are runtime-private. Scope drain releases the native
job for every Future and clears its native ID; an escaped Future destructor releases only its copied
terminal result/failure state.
`Future` and `TaskGroup` cannot be cloned or serialized: a duplicate would copy native job ownership
or an open scope capability. Ordinary PHP aliases to the same object remain valid.

While a Parallel parent scope or worker is active, user-origin `Fiber::suspend()` is rejected with
`Error`; a root scope or unsettled Future cannot be yielded to an unrelated resumer. The guard
covers direct calls, dynamic callable strings/arrays/descriptors, and callbacks dispatched by
the supported `array_map()`, `array_filter()`, `array_reduce()`, `array_walk()`,
`array_walk_recursive()`, and `usort()` higher-order builtins. The callback guard runs at its PHP
argument position: earlier
operands are evaluated in source order and owned temporaries are released if rejection throws;
operands to the right are skipped. Thus an `array_filter()` input is evaluated before its callback
is checked, while an `array_map()` callback (the first argument) is checked before its input arrays.
`preg_replace_callback()` evaluates its pattern before checking the callback and skips its subject
after rejection, with a shared descriptor-invoker backstop.
Ordinary callbacks remain callable. Async's scheduler-owned `await()`,
`reschedule()`, `sleep()`, and descriptor waits remain valid because they use trusted yield sites;
raw user suspension is not an Async scheduler wakeup.

Admission is bounded, but the v1 FIFO submission queue itself is unbounded: ordinary saturation
queues work in submission order rather than rejecting it. The current runtime has seven worker
slots behind the main context, but that count is an implementation detail, not a v1 concurrency
promise. `TaskFailureKind::ContextUnavailable` is reserved for a different production failure: an
admitted worker's configured context-acquire callback returns null. It is not an overflow signal.

`TaskGroup` itself is scope-bound and never has post-scope authority. A return that the checker can
prove contains the root `TaskGroup` is rejected. The runtime also marks the group closed when
`run()` has drained its children: every public group operation, including compiler-lowered
`spawn()`, checks that marker before it can create work. If an untyped or dynamically erased path
still reaches the closed object, it throws `Error`; it cannot submit a detached worker.
After a cancellation request while the scope is still open, `spawn()` also throws `Error` before it
creates a native job; late admission is not part of v1.
`run()` explicitly invokes idempotent cancel/drain/close cleanup from its `finally` path before
releasing the parent-scope guard. Retaining an alias to the TaskGroup therefore cannot defer closure;
the object destructor calls the same cleanup helper as a last-reference fallback.

## Transfer contract

`TaskGroup::spawn()` accepts a variadic argument list without a Fiber-derived
seven-argument ceiling. The worker receives a copied transfer payload; every argument
still has to satisfy the Parallel transfer/serialization contract and its resource limits.

Workers never receive a PHP pointer from the parent arena. Arguments, captures, and successful
results are copied through a versioned data-only envelope. The supported v1 value set is `null`,
booleans, integers, floats, binary strings, and recursively transferable indexed or associative
arrays. Objects, resources, pointers, buffers, callables, unresolved `mixed`, reference-aliased
arrays, and cyclic arrays are rejected at compile time.

`Elephc\Async\Cancellation` is the sole reference-like exception. It is not serialized: the worker
receives a fresh local proxy object bound to its native job flag; object identity (`===`) is not
preserved across the context boundary, and Cancellation values cannot be returned through Future
results. Both an explicit argument and a
`use (&$cancellation)` capture can call `isRequested()` or `throwIfRequested()` cooperatively.
This is a slot-level exception: the token must be a direct task argument or a direct closure
capture. Cancellation values nested in arrays or unions, and Cancellation task results, are rejected
by the checker before serialization. Other arrays still transfer recursively when every leaf uses
the ordinary scalar-and-array value contract.

Variadic task closures are currently rejected explicitly; the ordinary variadic `spawn(...$args)`
surface remains available for fixed-signature tasks. This avoids silently repacking a variadic tail
with different PHP semantics.

Task closures created from a statically known user PHP function or builtin are supported. A
user-declared native function is rejected unless it has an explicit worker-safety contract; `atoi()`
is currently the only such external. Externals that accept PHP callbacks are rejected because native
code can re-enter PHP outside the analyzed call graph. Receiver-bound closures and static-method
closures are rejected: the v1 worker ABI does not transfer an object receiver or a late-bound class
context.

Worker tasks and their statically analyzed callees also may not access process-global PHP
globals, static locals/properties, or process-wide runtime settings (working directory, environment,
umask, timezone, BC scale, or iconv default encodings). The current runtime also rejects APIs whose
mutable scratch is not yet context-owned: `json_*`, `stream_context_*`, `strtotime()`, enum case
singletons (`E::Case`, `E::cases()`, `E::from()`/`tryFrom()`), and the
`date()`/`gmdate()`/`getdate()`/`localtime()` conversion paths. These guards cover reads as well as
writes, so a parent changing the environment or timezone cannot race a worker reading it. Unknown
callback targets are rejected too. Ordinary local filesystem paths remain usable, but a path
operation that may dispatch through the process-global user stream-wrapper registry is rejected: a
literal URL-wrapper scheme is rejected, and a dynamically unknown path is treated conservatively.
Registering, unregistering, or restoring a user wrapper; inspecting the process-wide wrapper/filter
registries; and registering or attaching a user stream filter are also rejected inside a worker.
Compiler-private `elephc_parallel_*` bridge functions are not callable from PHP source, including
through runtime string callbacks.

A Parallel worker may not enter `Elephc\Async\run()` in v1. The checker rejects direct and
statically known transitive calls before codegen. When a Parallel program injects the Async surface,
it also prepends a worker-context guard to `Async\run()` itself, so a runtime-resolved callable such
as `call_user_func("Elephc\\Async\\run", ...)` fails closed with `Error` before a scheduler is
created. The future explicit cross-domain adapter is the only path that may add that composition.

`Elephc\Async\run()` is supported inside the parent root body passed to `Parallel\run()` only when
that parent body was entered from the main execution context, with no Async scheduler active. It
still executes in the main context, not in a worker, and waits synchronously for its nested Async
scope before the Parallel root continues. If a Parallel parent root is itself called from an Async
task, its body still runs in that task's Fiber; a further `Async\run()` then fails with
`Error("Elephc\\Async\\run() cannot be nested or called from a Fiber")`. It does not create a
scheduler inside an isolated arena.

A second `Parallel\run()` from that same parent root is rejected in v1, including through dynamic
dispatch. The runtime reports `Error` with
`Elephc\Parallel\run(): nested Parallel scopes are not supported in a parent root in v1`; this
avoids a reentrant executor and any pool-admission deadlock.

Likewise, nested `Parallel\run()` is rejected by the runtime worker guard even through dynamic
dispatch. The worker's parent sees `TaskFailure` with `kind() === TaskFailureKind::PhpThrowable`,
`remoteClass() === "Error"`, and message
`Elephc\Parallel\run(): nested Parallel scopes are not supported inside a Parallel worker in v1`.

`Parallel\run()` is supported inside an ordinary user-created Fiber. It runs synchronously in that
Fiber and drains its scope before returning. While a parent scope is active, a user-level
`Fiber::suspend()` on the same OS thread is rejected before its arguments are evaluated; this also
applies to user Fibers running in a Parallel worker. For a dynamically selected callback, Elephc
evaluates the callback expression once, checks its target, and rejects `Fiber::suspend` before
evaluating the invocation arguments. This covers callable strings, callable arrays, and first-class
callable descriptors. The caller cannot yield live `TaskGroup` or unsettled `Future` handles to an
external Fiber resumer: only Futures settled by scope drain may escape `Parallel\run()`.

`Parallel\run()` is also allowed inside an Async task, with the documented blocking warning. The
Async scheduler's own `await()`, `reschedule()`, `sleep()`, and descriptor-wait suspension points
remain supported and are resumed by that scheduler; user-authored raw `Fiber::suspend()` is not an
Async wake operation. Direct and dynamically resolved user calls to `Fiber::suspend()` inside an
Async task fail immediately with
`Error("Fiber::suspend() is not a scheduler wakeup inside an Elephc Async task")`.
A nested Async root
inside the Async task remains rejected by `Async\run()`'s any-Fiber guard.

When Parallel is linked, a runtime-resolved `Fiber::suspend` callback inside an Async task is also
preflighted even after the Parallel scope has ended; the callback raises the Async-specific `Error`
before dispatch. Direct static suspension hits the same Async guard.

`Parallel\TaskGroup` cannot be constructed by user code: its constructor is private and `run()` is
the only v1 creator. This keeps job ownership and scope closure attached to the structured parent.

## Cancellation and failures

The first root or child failure requests the group token, cancels queued work, signals running jobs,
and drains every admitted worker. Each job's native cancellation bit is read and written under the
same job mutex. For a still-running job, a worker observation whose read linearizes after the
parent's `cancel()` returns sees the request. A worker that passed its explicit check before the
request can continue until its next cancellation point; running PHP is never forcibly interrupted.
Queued or already terminal jobs keep their own terminal outcome and are not restarted.
`TaskGroup::cancel(?Throwable $reason)` follows the same group-wide path. Within the parent scope,
an already acquired `Cancellation` token reports the request and `throwIfRequested()` throws a
`CancelledException` whose `reason()` is that parent-side Throwable. No worker ever receives it:
workers see only the requested bit and a local cancellation with `reason() === null`. Scope close
clears the parent reason, and no independent reason accessor exists. An escaped parent token remains
observation-only after scope close: `isRequested()` retains its boolean state, while
`throwIfRequested()` yields a fresh cancellation with `reason() === null`.

After a normal root return, `Parallel\run()` returns that root value exactly when every failed
child was already observed through `Future::join()`. Successful and Cancelled children never block
that return; nor does an observed genuine worker failure. An unobserved failed child instead
causes `TaskGroupFailure`. Thus deliberate cancellation alone is neither `TaskFailure` nor
`TaskGroupFailure`.

A worker `Throwable` never crosses arenas. The parent reconstructs `TaskFailure` from a data-only
failure envelope containing `TaskFailureKind`, remote class, message, code, source location, and any
available compiled frames. Unobserved failures leave the scope as one `TaskGroupFailure`, ordered by
submission. A simultaneous root failure is preserved separately by `rootFailure()`; a lone root
Throwable is rethrown unchanged. `failures()` contains exactly the child Futures that reached the
Failed phase and whose failure was not previously observed through `join()`, in submission order.
It includes a genuine worker failure even if the group's cancellation request was already made;
Cancelled terminal outcomes are never `TaskFailure` entries. A queued job becomes Cancelled at the
request. A running job becomes Cancelled when its worker exits after observing that request without
already publishing a result or a genuine failure; an uncaught local `CancelledException` is this
Cancelled path, not a `TaskFailure`. A result or a non-cancellation Throwable that wins publication
while the job is still Running remains its own terminal outcome.

For a Parallel fail-fast request, workers never receive a parent Throwable. A root Throwable is
kept parent-local while the scope drains; a child failure triggers the native requested bit without a
shared reason. The first request is never overwritten by a later `cancel(?Throwable)`, and no
parent user code executes while a scope drains. Every worker-side `CancelledException` therefore
has `reason() === null`.

`TaskFailure` extends `RuntimeException` and exposes `kind()`, `remoteClass()`,
`remoteFile()`, `remoteLine()`, and `remoteFrames()`. A `TaskGroupFailure` exposes
its ordered child failures through `failures()` and any simultaneous root exception
through `rootFailure()`. `remoteLine()` is `0` when the failure kind has no source location;
`remoteFile()` and `remoteClass()` are then `null`.

Calling `Parallel\run()` from an Async task, like calling `Future::join()` there, is synchronous:
that call does not return until its Parallel scope drains. Async can still run tasks at explicit
scheduler-owned yield points in the body, including `await()`, `reschedule()`, `sleep()`, and
descriptor waits; Parallel work is not implicitly offloaded or preemptive. The compiler warns for a
directly proven Async task call, so applications should account for the synchronous scope duration.

## Execution policy

The current context pool contains eight internal slots, one reserved for the main context. This is
an implementation detail, not a public concurrency promise. Excess work remains threadless in a
FIFO queue until a context is available. V1 creates one nonpersistent platform thread per admitted
task; `Parallel\Runtime`, persistent pools, detached tasks, channels, and cross-domain Future/Awaitable
conversion remain outside this surface.

`Elephc\Parallel` is an Elephc extension and is unavailable under `--strict-php`.
Parallel execution requires a host-driven worker-thread contract and is currently unsupported for
iOS device and Simulator library targets. A program that reaches `Parallel\run()` on those targets
fails compilation with a target-specific diagnostic; `--check` remains available for analysis.

See [`examples/parallel-scheduler/main.php`](../../examples/parallel-scheduler/main.php).
