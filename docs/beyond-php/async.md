---
title: "Structured Async Tasks"
description: "Run cooperative Fiber tasks in a structured scope with Elephc Async."
sidebar:
  order: 10
---

`Elephc\Async` provides structured cooperative tasks on top of Elephc's native
PHP Fibers. Tasks run on one OS thread and share the caller's PHP runtime
context, object identity, resources, globals, and heap arena.

The current implementation includes a descriptor-readiness reactor, but no
blocking-I/O offload, parallel threads, detached tasks, or preemption.

## Run a structured scope

```php
<?php

use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$results = run(function (TaskGroup $tasks): array {
    $left = $tasks->spawn(fn (int $value): int => $value * 2, 21);
    $right = $tasks->spawn(
        fn (string $name): string => "hello " . $name,
        "elephc",
    );

    return [$left->await(), $right->await()];
});

echo $results[0] . "|" . $results[1];
```

Output:

```text
42|hello elephc
```

`run()` creates and drives the root task scope. Its closure receives the only
`TaskGroup` that may create children in v1. The call returns the root closure's
value only after every spawned child has completed or failed, even if the root
closure returns without awaiting a child explicitly. Children may use that same
group to spawn descendants; those descendants remain owned by the root scope.

An escaped `TaskGroup` has no post-scope authority. Once `run()` has detached its
scheduler, every public group operation — including `cancel()` and
`cancellation()` — throws `Error` instead of observing or mutating ended scope
state. A `Cancellation` token obtained while the scope was active remains a
read-only value; it does not regain scheduling or cancellation authority. It retains the request
state and, when the scope was cancelled with a reason, `throwIfRequested()` continues to expose that
same reason after scope close. The token and reason remain ordinary PHP references in the containing
runtime context; closing Async tears down scheduler records, not the context arena.

`run()` must be entered from the main execution context. Calling it from an
Async task or any other Fiber raises `Error`; libraries compose by accepting the
current `TaskGroup`, not by creating a nested event loop.

## Await task results

`TaskGroup::spawn(callable $task, mixed ...$args): Awaitable` appends a task to
the scheduler's FIFO ready queue without running it inline. Its variadic arguments are
not capped to Fiber::start()'s current seven-slot transport. Every child uses a static
zero-argument runner that invokes the callable through `call_user_func_array()` with
the preserved argument array when the child Fiber runs. Static arguments, named
arguments, and dynamic spreads share the normal call rules, subject to process
resources. The runner adds an observable
closure frame to `debug_backtrace()`; Async does not promise a stable stack-frame shape.
`Awaitable::await()` returns an already completed result immediately while its owning
`run()` scope is active. When the target is still running, it suspends the current
Fiber; completion appends the waiter to the ready queue. Awaitables are scoped handles:
when `run()` closes its scheduler, escaped handles become invalid and both
`await()` and `isComplete()` throw `Error`. Unlike Parallel `Future`, an Async
Awaitable does not carry copied results beyond the owning scope.

Once cancellation has been requested, `spawn()` throws `Error` before it creates a child. It never
admits a late task merely to cancel it later.

Multiple tasks may await the same `Awaitable`. Each waiter observes the same
result or the same task-specific exception.

`Awaitable::isComplete()` reports whether the task has reached a terminal state. It returns
`true` after a normal return, an escaping failure, or cancellation; it does not observe, consume,
or rethrow a stored outcome.
Outside an Async task, `await()` can inspect a terminal target synchronously while the scope is
active; an unfinished target instead throws `Error` without registering a waiter.
An unfinished task that waits on itself is rejected, and a scope with live tasks
but no runnable task fails with an Async deadlock error instead of spinning.

Calling raw `Fiber::suspend()` from user Async code does not register a scheduler wake source.
Direct calls and dynamic callables resolving to it raise
`Error("Fiber::suspend() is not a scheduler wakeup inside an Elephc Async task")` before suspension.
Use `Awaitable::await()` or the owning `TaskGroup`'s `reschedule()`, `sleep()`, or descriptor-wait
methods. Live tasks with no registered wake source still fail closed with
`Error("Elephc Async deadlock: tasks are live but none are runnable")`.

`TaskGroup` and `Awaitable` are implementation-visible prelude classes, but they are not ambient
authority or alternate scope-entry APIs. A manually constructed instance carries only the scheduler,
scope, state, and task identifier explicitly supplied to its constructor; it never discovers or
attaches to a currently active `run()` scope. Such an instance therefore cannot cancel, drain, or
observe a live scope unless code already possessed that scope's private references. The sole v1
operations that create an active root and its children remain `run()` and that root group's
`spawn()`. Async construction only stores explicitly supplied same-context references; unlike a
Parallel `TaskGroup`, it does not enter thread-local parent-scope state. Manually constructed handles
have no additional behavior guarantee.

`TaskGroup::reschedule()` voluntarily suspends the current task and appends it to
the FIFO tail. It is an explicit fairness point; it does not create parallelism
or preempt a task that never calls it.

`TaskGroup::sleep(float $seconds)` suspends only the current task until a
monotonic deadline. Runnable tasks continue first. When every live task is
sleeping, the scheduler blocks the OS thread only until the earliest deadline;
equal deadlines wake in task-creation order. Zero seconds behaves like
`reschedule()`; positive and negative IEEE-754 zero both take this path. Strictly negative,
non-finite (`NAN`/`INF`), and durations that exceed
the representable signed nanosecond-deadline range raise `ValueError` before a
timer is registered.

## Cancellation

`TaskGroup::cancellation(): Cancellation` returns a read-only observation token.
The token exposes `isRequested()` and `throwIfRequested()`; cancellation authority
stays on the owning group through `TaskGroup::cancel(?Throwable $reason = null)`.
The first request wins, and an optional reason is available from
`CancelledException::reason()`. A fail-fast Async request uses the first escaping task Throwable as
that reason, so cooperative siblings in the shared context can inspect it at their cancellation
point. A prior `cancel(null)` is still the first request: a later cleanup failure does not replace
its null reason.

Cancellation is cooperative. An unstarted child is completed without invoking its
body. A child parked in `await()`, `sleep()`, or `reschedule()` is made runnable and
throws `CancelledException` from that cancellation point. The child may catch it,
perform cleanup, and return normally: that normal return is a Completed task value. If the
request-paired `CancelledException` escapes the task, it is terminal Cancelled; a user-thrown
`CancelledException` without an active group request is an ordinary failed task. Awaiting a
terminal-cancelled child throws
`CancelledException` carrying the scheduler's same first-request reason, including when the waiter
is the root task. The root task itself is not implicitly cancelled, so it remains
responsible for leaving the structured scope after all children settle. The root may continue
through its own `reschedule()`, `sleep()`, and descriptor waits after it requests cancellation. If
it awaits a child that has become Cancelled, that `await()` throws the request-paired
`CancelledException` at the root's call site. If the root catches it, the root may return normally;
if it escapes the root, the scheduler records it as the root failure. After child cleanup, `run()`
rethrows that same exception object with its first-request reason when it is the first unobserved
failure in occurrence order; an earlier unobserved child failure remains the propagated outcome. A direct
`Cancellation::throwIfRequested()` call likewise throws at the explicit call site.
If that root returns normally after requesting cancellation, `run()` drains the
cancelled children and returns the root value; cancellation is not aggregated as
an unobserved task failure.

A running child observes cancellation only when it reaches a cancellation point or
calls `Cancellation::throwIfRequested()` explicitly. A CPU-bound task that never
cooperates cannot be forcibly interrupted.

## Await descriptor readiness

```php
$readable = $tasks->awaitReadable($stream, timeout: 1.0);
$writable = $tasks->awaitWritable($stream, timeout: 1.0);
```

Both methods accept a native PHP stream resource or an integer descriptor. They
return `true` when libc `poll()` reports readiness, hangup, or an OS condition
that the following stream operation must inspect. They return `false` only when
the optional monotonic timeout expires. `null` waits without a deadline, `0.0`
performs a non-blocking probe. Raw descriptors outside `0..=2147483647` raise
`ValueError` before duplication or registration, and non-resource/non-integer values raise
`TypeError`. Timeouts must be finite, non-negative, and small enough to form a
signed monotonic deadline; negative, non-finite, or out-of-range values raise
`ValueError` before descriptor registration.

Pass PHP stream resources directly. `(int) $stream` and `get_resource_id()`
expose the PHP resource identifier, which is not guaranteed to be the underlying
OS descriptor. The integer form accepts integers in `0..=2147483647` as raw OS
descriptors; the runtime cannot establish their provenance, so callers must
ensure they really are open descriptors (typically from Elephc FFI or an
equivalent native boundary). Negative and out-of-range descriptors are rejected
before they reach `poll()`, where POSIX would ignore negative `pollfd.fd` entries
and a null-timeout wait could otherwise block forever.

Descriptor waits are cancellation points. They do not read or write data and do
not silently change the descriptor's blocking mode. Set the stream to
non-blocking mode before an operation that could otherwise block after partial
progress. User stream-wrapper callbacks, regular-file offload, DNS, TLS, curl,
and databases are not made asynchronous by this API.

The v1 reactor uses a dependency-free, level-triggered libc `poll()` adapter.
Registrations are append-only within one scope, ready events preserve
registration order, descriptors above 63 are supported, and the OS thread blocks
only when the FIFO ready queue is empty. The internal backend boundary can later
use epoll or kqueue without changing this PHP surface.

Within one drive turn, cancellation wakeups are appended first, then expired `sleep()` timers in
task-creation order, then expired I/O deadlines and non-blocking `poll()` readiness in registration
order. If a blocking `poll()` call itself returns readiness, that ready batch is appended in
registration order before the executor rechecks timer and I/O deadlines. This pins the only
cross-category ordering v1 exposes.

## Failures

An exception escaping a child Fiber is retained by the scheduler. Awaiting that
task throws the exception in the awaiting task and marks that failure observed.
If the caller catches it, the root scope does not throw the same failure again.
Multiple waiters still receive the same task-specific exception.

The scheduler is fail-fast. An exception escaping either the root or a child
requests cancellation of every live child. The scope waits for their cleanup
before leaving `run()`. Fail-fast cancellation is not rolled back when the
original failure is caught; a later cleanup failure remains independent. Scope
exit throws the first failure, in occurrence order, that was not observed through
`await()`, so dropping an `Awaitable` cannot silently discard its failure. The
root task participates in that same occurrence order: a child failure that
occurs first wins over a later root failure, while a root failure that occurs
first is the propagated exception.

## Cooperative execution

Only one task runs at a time. A task yields when it awaits an unfinished task;
ordinary PHP calls remain synchronous. In particular, `sleep()`, `fread()`, PDO,
curl, and arbitrary FFI calls do not become asynchronous merely because they run
inside `Elephc\Async`.
Likewise, `Parallel\Future::join()` and `Parallel\run()` block the same OS thread. Async timers keep
their monotonic deadlines and registered descriptors remain owned by the reactor, but no Async task
is dispatched and no readiness poll runs until the blocking call returns. The scheduler then
processes expired deadlines and current descriptor readiness in its normal order.

Exact monitoring records task lineage, runnable-queue delay, running time, blocked
time, wake reasons, and terminal state. The transition hook remains dormant when
monitoring is not linked or no capture window is active. Higher-level asynchronous
adapters remain future work; CPU-bound tasks that do not call `reschedule()` and all
blocking calls keep the scheduler's single thread occupied.

## Strict PHP

`Elephc\Async` is an Elephc extension. Tagged PHP compiled with `--strict-php`
does not receive the scheduler prelude and reports its classes/functions as
unknown. Dynamic eval and its introspection surface likewise do not expose Async
under strict mode. Tagless `.lfc` source remains extension-enabled.

See the complete runnable example in
[`examples/async-scheduler/main.php`](../../examples/async-scheduler/main.php).
