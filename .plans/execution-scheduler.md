# Execution Scheduler — design and consensus plan

Status: Candidate 62 is the sole current implementation/review draft; its code is not frozen. Candidate 61 was rejected after independent Codex reviews and reproduced array-ABI regressions. Async TaskGroup::spawn retains unbounded arguments and the zero-argument runner. Shared PHP-array transport/consumer correction, live-fix validation, current-tree target evidence, independent Codex/Kimi/GLM/DeepSeek acceptance and publication remain open. No historical verdict counts as current consensus.

Related foundation: [`sandbox-threads.md`](sandbox-threads.md). That plan owns `_rt_ctx`,
isolated arenas, cross-context transfer, and the sandbox-thread implementation. This plan owns
task semantics, scheduling policy, the Async/Parallel public API, reactor integration, and the
boundary between cooperative and parallel execution.

## Checklist

- [x] Inventory the existing Fiber, web-loop, monitoring, PCNTL safe-point, and `_rt_ctx`
      foundations.
- [x] Separate process scheduling, cooperative task scheduling, parallel execution, and durable
      job scheduling.
- [x] Preserve the locked `Elephc\Async` / `Elephc\Parallel` and `Awaitable` / `Future`
      distinction.
- [x] Draft a minimal structured-concurrency API for review.
- [x] Draft task lifecycles, scheduler invariants, cancellation, failure, and liveness semantics.
- [x] Define a deliberately simple cooperative v1 scheduling policy and a Linux-inspired future
      preemption path.
- [x] Identify the I/O, web, monitoring, target, and runtime-context integration boundaries.
- [x] Record implementation slices and acceptance gates; the maintainer opened the implementation
      gate for the locked v1 scope on 2026-09-13.
- [x] Lock `run()` as the root structured boundary and `TaskGroup::spawn()` as the only public v1
      task-creation operation (maintainer consensus, 2026-09-13).
- [x] Resolve every item under "Consensus questions" with the maintainer.
- [x] Freeze the first review candidate and record its SHA-256 in
      `execution-scheduler.sha256` (`f3b18f91c6c3381ebb871d23dbb5e68d6df33d85e427eaef7f98db808c8bc7a5`).
- [x] (Historical first review cycle only; not the current review round) Obtain three independent reviews against the first frozen hash
      `f3b18f91c6c3381ebb871d23dbb5e68d6df33d85e427eaef7f98db808c8bc7a5` (historical first cycle;
      this does not satisfy any later exact-hash review round).
- [x] Reconcile every blocking finding from the first exact-hash review in specification, code,
      and focused acceptance evidence (2026-09-21), plus the second-review cancellation and
      TaskGroup-scope findings (historical rounds only; this item does not close later candidate
      implementation or review gates).
- [x] Freeze the reconciled specification under the current companion SHA-256 in
      [`execution-scheduler.sha256`](execution-scheduler.sha256), updated after every material amendment.
- [ ] Complete three independent reviews against that exact new hash.
- [x] Record the accepted decisions in Kioku, superseding any conflicting packet.
- [x] Begin implementation only after the maintainer explicitly opens the implementation gate;
      that gate applies to the locked v1 scope, not to deferred library or backend work.

## Problem statement

Elephc has two complementary concurrency mechanisms in progress:

1. PHP Fibers provide stackful cooperative execution inside one runtime context.
2. Sandbox threads will execute PHP in parallel, with one `_rt_ctx` and one isolated arena per
   thread and explicit deep-copy transfer across the boundary.

Neither mechanism alone defines task ownership, scheduling, wakeups, cancellation, failure
propagation, timers, or I/O readiness. The missing component is an execution model whose public
surface stays structured while its internal scheduler can choose a local Fiber or an isolated
thread according to the execution domain.

This is not a durable job scheduler. Cron expressions, persisted queues, distributed leases,
retries after process death, and wall-clock calendar execution belong in a service or library
built on top of Elephc, not in the compiler runtime.

## Status vocabulary

Labels in this document are normative:

- **LOCKED** — inherited from an already arbitrated decision in `sandbox-threads.md`; changing it
  requires an explicit reversal and rationale.
- **PROPOSED** — the current recommendation, ready for maintainer review but not yet authoritative.
- **OPEN** — multiple viable choices remain; implementation must not select one implicitly.
- **DEFERRED** — intentionally outside v1, with no stub or accidental partial behavior.

## Candidate scope and implementation gate

This candidate covers the shipped Async and Parallel v1 runtime described by the **LOCKED AND
IMPLEMENTED** sections: structured root scopes, task groups, cancellation, failures, timers,
descriptor waits, isolated worker execution, and their ownership/target diagnostics. The
maintainer opened this implementation gate on 2026-09-13 for that scope. A **DEFERRED** library
helper, scheduler policy upgrade, web-concurrency design, or native backend replacement is not an
unfinished v1 requirement and cannot silently acquire partial behavior under this gate.

The historical review entries below explain how the candidate reached its current state; they do
not mark the current exact-hash round complete. The unchecked checklist item is intentional: only
three complete `LOCK` + `ACCEPT` + no-findings/no-questions transcripts for the current companion
hash close this candidate and permit a push.

## Locked boundaries inherited from sandbox threads

### Two execution domains — LOCKED

`Elephc\Async` runs cooperative Fibers on one OS thread and in one `_rt_ctx`. Values are ordinary
same-context PHP values: objects, resources, references, and mutations keep their normal identity
and visibility.

`Elephc\Parallel` runs work on sandbox Rust threads. Each worker owns its `_rt_ctx`, heap arena,
exception chain, Fiber state, stack limits, GC accounting, scratch buffers, and resource tables.
Only statically transferable values cross the boundary, by deep copy.

### Two result handles — LOCKED

`Elephc\Async\Awaitable<T>` and `Elephc\Parallel\Future<T>` remain distinct, with no implicit
conversion. The type must continue to reveal whether an operation shares the caller's arena or
crosses into an isolated one.

### Structured ownership — LOCKED

A task belongs to a lexical/dynamic task scope. A scope cannot complete while a child is still
running. Failure and cancellation propagate through the task tree, and cleanup completes before
the scope returns or throws. This is an exit-order guarantee, not a progress/preemption guarantee:
blocking user calls may keep a scope open indefinitely and delay all scheduler-owned cleanup.
Detached tasks are outside v1.

### Cross-context transfer — LOCKED

Parallel arguments, captures, results, and failures cannot smuggle a pointer into another arena.
Scalars and recursively transferable arrays are copied. Objects, resources, borrowed values,
ordinary by-reference captures, and dynamically unresolved closure targets are rejected by the AOT
checker. The Async `Cancellation` observation token is the sole deliberately shared cross-context
capability and the sole by-reference capture exception: it is permitted only as a direct task argument
or a direct closure capture (including `use (&$cancellation)`) and is reconstructed as a worker-local
job proxy, never transferred as a PHP reference cell; a by-reference capture therefore observes the
same native job request bit without aliasing or mutating the parent's variable/object storage.
Reconstruction creates a fresh PHP
`Cancellation` object in the worker; `===` identity with the parent object is not preserved, and the
token cannot be returned or nested in a Future result to cross the boundary back.
This direct token argument/capture (including its named by-reference capture) is the sole exception
to v1's by-reference-capture and object/resource transfer prohibitions; every other ordinary
by-reference capture, object, or resource remains rejected for worker transfer. Each direct token
occurrence in a job's arguments/captures is reconstructed as a fresh proxy over that job's native
request bit; multiple occurrences share cancellation state but do not promise PHP `===` identity with
one another or with the parent token.

### Scheduler placement — LOCKED

The scheduler is an internal execution-policy parameter, not a third public concurrency API.
Async and Parallel expose domain-appropriate operations through their own namespaces and handles.

## Layered architecture — LOCKED AND IMPLEMENTED

```text
Application code
    |
    v
Structured scopes, cancellation, Awaitable/Future results
    |
    +-------------------------------+
    |                               |
    v                               v
Async executor                  Parallel executor
same _rt_ctx                    isolated _rt_ctx per worker
Fiber task                      sandbox-thread task
ready queue + reactor           unbounded FIFO queue, bounded admission
timers and I/O wakeups          context/arena acquisition
    |                               |
    +---------------+---------------+
                    v
        common task events and monitoring
```

The common layer owns task identity, parent/child relationships, cancellation lineage, completion
status, and monitoring vocabulary. It must not pretend the two result-storage models are the same.

### Runtime ownership rule — LOCKED AND IMPLEMENTED 2026-09-16

Native scheduler/reactor boundaries store numeric task IDs, deadlines, descriptor interests, and
status flags only. They do not own PHP values, Fiber pointers, object pointers, or Throwable
pointers. The compiler-owned Async PHP kernel keeps Fibers, results, and Throwables inside the
owning context; the Parallel bridge retains only copied envelopes and native job state.

Task IDs contain an index and generation. A stale readiness event, cancellation callback,
or file-descriptor reuse must not wake a newly allocated task that happens to occupy the same slot.
Duplicate wakeups are idempotent.

### Compiler/runtime/bridge split — LOCKED AND IMPLEMENTED 2026-09-16

The public Async and Parallel types are Elephc extension classes and functions emitted from
compiler-owned AST builders with retained parse-parity oracles. Their native hooks use typed builtin
contracts, so name lookup, signatures, strict-PHP visibility, documentation, and backend support
stay explicit without raw source injection.

The AOT checker owns the rules it can prove before execution:

- root-scope and task closure signatures;
- Parallel transferability of arguments, captures, and declared results;
- forbidden by-reference and dynamically unresolved callable forms;
- target support and statically provable blocking cross-domain joins.

Compiler-owned AST preludes own the PHP-visible structured APIs. EIR owns the native Fiber
operations used by Async and the typed `ParallelSpawn` operation; ordinary Async scope policy stays
visible as generated PHP control flow. PHP names do not select assembly emitters.

The emitted runtime owns Fiber switching, descriptor polling, monitoring transitions, and
safe-point state. The generated Async kernel owns same-context task records and result/Throwable
ownership. The Parallel Rust bridge owns OS-thread admission and copied job envelopes. Native
boundaries exchange numeric IDs, copied bytes, arena descriptors, and status codes; they do not
transfer live PHP pointers.

Magician support remains explicit: strict mode hides the extension surface from dynamic eval, and
eval never gains an independent scheduler or an untracked runtime stub.

## Async task model

### Lifecycle — LOCKED AND IMPLEMENTED 2026-09-16

```text
Created -> Runnable (never started) -- FIFO dispatch --> Running
Running -- normal return --> Completed(value)
Running -- uncaught Throwable --> Failed(error)
Running -- await --> WaitingTask
Running -- sleep / I/O wait --> Sleeping / WaitingIo
WaitingTask -- awaited child terminal completion (including Cancelled) --> Runnable (started)
Sleeping / WaitingIo -- timer / readiness --> Runnable (started)
WaitingTask / Sleeping / WaitingIo -- cancellation wake (non-root; remove registration) --> Runnable (started)
Runnable (started) -- FIFO dispatch (pending request is checked after entering Running) --> Running
Runnable (never started) -- cancellation before first activation --> Cancelled
Running -- explicit cancellation point sees request --> CancelledException delivered in task Fiber
CancelledException delivered -- escapes child --> Cancelled
CancelledException delivered -- caught by user --> Running (continues at catch)
CancelledException delivered -- caught handler returns --> Completed(value)
```

The diagram shows task phases, not cancellation-bit state. A cancellation request never preempts the
currently Running Fiber. A never-started child is terminalized Cancelled without invoking its body.
For an already-queued started child, the request is orthogonal pending metadata: it remains Runnable
at the same FIFO position until selected. A cancellation wake removes the child's wait registration
and makes it Runnable; at FIFO dispatch the scheduler first marks it Running, then resumes its saved
continuation. If a cancellation request is pending, the continuation's explicit check observes it
before executing the next ordinary PHP statement. Thus the lifecycle transition to Running precedes
the cancellation check; the request bit is not checked before FIFO selection.
The single-threaded executor never resumes a second
Fiber concurrently with the current Running task. A request-paired `CancelledException` escaping a
child becomes Cancelled; if caught, execution continues normally from the catch block and later
returns to Completed or throws to Failed. An unpaired user-thrown `CancelledException` follows
ordinary Throwable semantics: uncaught means Failed, caught means normal catch-block continuation.
The root is excluded from group cancellation wakeups and never has the child-only Cancelled terminal
phase; an explicitly awaited/thrown request-paired exception escaping the root is an ordinary root
failure. No cancellation path starts a child body for the first time after its request.
`Running` includes both the root body Fiber and spawned child Fibers. Cancellation does not preempt
running PHP: the task observes it only at an explicit cancellation point. Candidate-30's internal
Async callback-safety flag is set around each scheduler `start()`/`resume()` of either root or child
and cleared after normal return or the scheduler's caught-Throwable path.

Invariants:

- Exactly one task is `Running` per Async scheduler.
- A task appears at most once in the ready queue.
- A wait registration is removed before the task becomes runnable.
- Readiness callbacks enqueue a task; they never resume a Fiber inline or reentrantly.
- A completed, failed, or cancelled task never becomes runnable again.
- Results stay owned by the task's `_rt_ctx` until the scope and all result handles release them.
- Awaiting the same completed Async result more than once is allowed and returns/throws the same
  logical outcome with normal refcount ownership for each consumer.
- Each `await()` returns the stored PHP value under ordinary PHP value semantics, not a deep clone:
  arrays retain copy-on-write behavior, while object results preserve object identity, so a mutation
  through one awaiter is visible to later awaiters of that same result. The task outcome retains its
  own reference until scope/result-handle release.

### Cooperative v1 policy — LOCKED AND IMPLEMENTED 2026-09-16

Use a deterministic FIFO ready queue in v1:

1. `spawn()` appends a new task and never executes it inline.
2. A running task continues until it completes, fails, cancels, or reaches an explicit suspension
   point.
3. A voluntarily yielding task returns to the tail of the queue.
4. Each drive turn appends cancellation wakes first, expired `sleep()` timers in task-creation
   order, expired I/O deadlines in registration order, then non-blocking `poll()` readiness in
   registration order. A blocking `poll()` result is appended in registration order before the
   executor rechecks elapsed timer/I/O deadlines. Equal deadlines preserve their applicable order.
5. When no task is runnable, the executor blocks in the reactor until the earliest timer or I/O
   event.
6. The root loop exits only when its structured scope has no live children and no referenced waits.

Weighted fairness or EEVDF-like accounting cannot make a non-yielding cooperative task fair. Adding
it before preemption would add clock reads and policy complexity while making a promise the runtime
cannot enforce. FIFO is therefore the honest v1 policy.

### Linux-inspired preemption path — DEFERRED

The useful Linux ideas are runnable/waiting states, separate mechanism and policy, execution-time
accounting, wakeup accounting, a reschedule flag, and later virtual deadlines. The OS continues to
schedule Elephc processes and threads across CPUs.

If cooperative starvation proves material, soft preemption may be introduced later:

1. A timer or accounting check sets a per-context `need_resched` flag.
2. Compiler-inserted safe points inspect that flag at loop backedges and selected call boundaries.
3. A safe point preserves every live register/value and yields to the local executor.
4. The policy may then use weighted virtual runtime, lag, and virtual deadlines inspired by Linux
   EEVDF.

The existing PCNTL async dispatch wrapper demonstrates that Elephc can preserve machine state at an
EIR instruction boundary. It is a starting point, not proof that arbitrary Fiber switching is safe
at every such boundary. Direct context switching from an asynchronous signal is forbidden: a signal
may interrupt allocator, COW, refcount, bridge, or exception machinery in an inconsistent state.

No user-visible `nice`, real-time class, deadline guarantee, or scheduler plug-in belongs in v1.

## Async public API

### Minimal kernel surface — LOCKED AND IMPLEMENTED 2026-09-16

The ownership shape of `run()` and `TaskGroup::spawn()` was locked by maintainer consensus on
2026-09-13. The complete implemented v1 surface is:

```php
namespace Elephc\Async;

function run(callable $body): mixed;
final class TaskGroup
{
    public function spawn(callable $task, mixed ...$args): Awaitable;
    public function reschedule(): void;
    public function sleep(float $seconds): void;
    public function awaitReadable(mixed $stream, ?float $timeout = null): bool;
    public function awaitWritable(mixed $stream, ?float $timeout = null): bool;
    public function cancellation(): Cancellation;
    public function cancel(?\Throwable $reason = null): void;
}

final class Awaitable
{
    public function await(): mixed;
    public function isComplete(): bool;
}

final class Cancellation
{
    public function isRequested(): bool;
    public function throwIfRequested(): void;
}

final class CancelledException extends \RuntimeException
{
    public function reason(): ?\Throwable;
}
```

Async `TaskGroup::spawn()` has no seven-argument cap. Every child starts a static zero-argument
runner retaining the callable and expanded variadic arguments; that runner invokes the callable
through `call_user_func_array()` inside the scheduled Fiber. Numeric and string argument keys are
preserved, including named variadic captures. Normal callable argument binding applies at task
execution. The runner adds an observable closure frame at every arity; Async does not promise a
stable `debug_backtrace()` frame shape. Parallel uses its independent copied-transfer ABI and also
has no Fiber-derived argument cap.

`run()` creates the root `TaskGroup`, invokes the body inside a root Fiber, drives the event loop,
and returns the body's value only after all descendants have settled. If the body returns while a
child remains live, `run()` keeps driving that child to a terminal state before returning or throwing;
no descendant detaches. The body must accept the root
group as its first argument; the checker rejects any incompatible closure signature:

```php
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$result = run(function (TaskGroup $tasks): array {
    $left = $tasks->spawn(fn (): string => fetch_left());
    $right = $tasks->spawn(fn (): string => fetch_right());

    return [$left->await(), $right->await()];
});
```

`Awaitable::isComplete()` is an outcome observation only: while the scope is live, it returns `true`
after normal return, failure, or cancellation and `false` for every non-terminal state. It never marks
a failure observed, releases it, or throws it. It is callable from outside an Async task while the
scope is live, including for a pending target; the task-context requirement applies only to
`await()` on a pending target. Like `await()`, after scope close it throws the
scope-ended `Error` rather than returning a terminal-state answer; the closed guard takes precedence
over the terminal phase, even when completion occurred before teardown. The observable matrix is
therefore:
live + terminal => `true`; live + non-terminal => `false`; closed + any target state => scope-ended
`Error`.
While its owning Async scope remains live, `Awaitable::await()` from outside an Async task is
permitted only after its target is terminal; it then returns or rethrows that stored outcome
synchronously. Calling it outside a task while the target is still pending (any non-terminal state
other than `Completed`, `Failed`, or `Cancelled`, including `Created`, `Runnable`, `Running`,
`WaitingIo`, `Sleeping`, `WaitingTask`, or an internal cancellation-delivery state) throws
`Error("Awaitable::await() must be called from an Elephc Async task")` without registering a waiter.
Awaiting the same terminal `Awaitable` repeatedly, including by the same task, is permitted and
returns the same logical result or rethrows the same stored `Throwable`; it is not a one-shot
consumption operation. Any failed await sets the task-global observed bit described below. An
outside-task await of a terminal Cancelled child is also permitted while the scope is live and throws
a fresh request-paired `CancelledException` from that scheduler's cancellation state, carrying the
first request reason (or null); it does not expose a worker-local exception or cross an arena. Every
Cancelled await constructs a fresh exception object, including repeated awaits by the same task; the
logical cancellation outcome and `reason()` identity/value remain the same, but exception object
identity is not preserved. For a Failed task, by contrast, every await rethrows the same stored
Throwable object.
After the event loop has drained the root and every descendant, `runRoot()` captures the root's
return value (on success) and calls `teardown()`. The first operation in `teardown()` sets the
scheduler's `closed` flag to `true`, before releasing any I/O registration, Fiber, outcome, or task
record. `throwFirstUnobservedFailure()` may enter this same teardown before rethrowing its selected
failure. From the exact synchronous write of `closed = true` onward, every escaped Awaitable is
invalid: both `isComplete()` and `await()` throw
`Error("Elephc Async Awaitable scope has ended")`, even if the referenced task had already become
terminal. Before that transition, while the scope is live, the preceding terminal/pending outside-task
rules apply. Async is single-threaded, so another thread cannot race an Awaitable call against this
transition. Any synchronous/reentrant observation before this write still sees a live scope with a
terminal target and follows the terminal outside-task rule; after the write, including destructor
re-entry caused by releasing a registration or task record, the scope-ended `Error` is thrown.
Thus a reentrant `isComplete()`/`await()` after the root and all descendants have terminalized but
before `closed = true` returns `true`/the stored terminal outcome respectively; the same call after
the write throws the scope-ended `Error`. No other thread can interleave this boundary.
Async handles do not copy terminal outcomes
out of the scheduler; this differs intentionally from `Parallel\Future`, whose post-scope copied
terminal state is part of its API.

`Async\TaskGroup` is scope-bound at runtime. Its scheduler-owning methods already resolve the
active `__ScopeState`; `cancel()` and `cancellation()` take that same guard before accessing the
cancellation state. After `run()` detaches the scope, every public group operation throws `Error`.
An already obtained `Cancellation` token remains observation-only and never regains group authority.
An escaped Async token retains its request state and original reason through ordinary PHP object
references in the containing runtime context; closing the scheduler does not tear down that context
or leave an arena pointer in the token. An escaped Parallel token also retains its request state, but
scope close clears its parent-only reason so a later `throwIfRequested()` carries `null`.

The change from the older sketch is intentional: `run()` drives a root scope and returns
its final value; `spawn()` returns an `Awaitable`. A function named `run()` returning a merely queued
handle obscures who drives the executor and who owns the child. This `run`/`spawn` ownership split is
now **LOCKED**.

`run()` creates the root task; `TaskGroup::spawn()` is the only public operation that creates child
tasks in v1. Every spawned task is owned by that group, and the group cannot complete while a child
remains live. Detached tasks are explicitly
excluded from v1. Once cancellation is requested, Async and Parallel reject a further `spawn()`
immediately when the synchronous `spawn()` call checks the request, before creating a task record or
native job; neither domain admits a late child merely to cancel it later. Async throws
`Error("Elephc Async TaskGroup cannot spawn after cancellation has been requested")`; Parallel throws
`Error("Elephc Parallel TaskGroup cannot spawn after cancellation has been requested")`. This is
**LOCKED**.

Only `Async\run()` may create a live scheduler root. The implementation constructor signatures are
`__Scheduler::__construct(__CancellationState)`, `__ScopeState::__construct(__Scheduler)`,
`TaskGroup::__construct(__ScopeState, __CancellationState)`, and
`Awaitable::__construct(__Scheduler, int $taskId)`. These constructors remain syntactically public
implementation details so ownership/GC probes can build isolated records; they retain only the
explicit references supplied by the caller and never discover the active scope. `__Scheduler::runRoot()`
is the authority-bearing entry: it is private and only the compiler-owned `Async\run()` function may
invoke it through a narrow friend channel. An ordinary direct or dynamically resolved user call to
`runRoot()` therefore fails PHP private-method visibility before driving any task. A manually
constructed scheduler/group may queue inert records but cannot enter a live root through supported
userland calls, and it cannot attach itself to a different active root. Reflection bypass is outside
the integrity guarantee. This differs from `Parallel\TaskGroup`, whose private constructor enters
thread-local parent-scope state and is accessible only to the compiler-owned `Parallel\run()` path.

Async accepts any `callable`, not only `Closure`: named functions and methods do not cross an arena
boundary, so Parallel's static transfer restriction would add no safety here. This is implemented
and pinned by prelude-contract parity.

`TaskGroup::sleep()` uses a monotonic deadline and suspends only the current task. It never delegates
to PHP's blocking `sleep()`. `TaskGroup::reschedule()` is an explicit fairness point and requeues the
current task. For a non-root task, `reschedule()` checks cancellation before yielding and immediately
after resumption, so a task repeatedly rescheduling observes a pending request at its next check;
the root keeps its documented exemption from these automatic checks. Zero seconds is a reschedule
point. Negative, non-finite (`NAN`/`INF`), or finite
durations that cannot fit the signed nanosecond monotonic-deadline range raise `ValueError` before
registering a timer. The name deliberately avoids PHP's reserved `yield` keyword.

### Library helpers — DEFERRED

`all()`, `any()`, `first()`, bounded `concurrently()`, and `series()` should initially be PHP/prelude
library functions implemented through `TaskGroup::spawn()` and `Awaitable::await()`. They are not
separate runtime primitives and are outside v1. `later()` remains internal until a concrete public
use case requires it.

### Scheduler nesting — LOCKED AND IMPLEMENTED 2026-09-13

Calling `run()` while an Async scheduler is active is rejected in v1 with a named error. Libraries
compose by accepting the current `TaskGroup` or by spawning into it, not by creating nested event
loops. Reentrant/nested reactors are a common source of ordering and lifetime defects.

The context-safe v1 guard rejects `run()` from any current Fiber, including a Fiber not created by
Async. This deliberately conservative rule avoids process-global current-scheduler state and keeps
the behavior well-defined until `_rt_ctx` owns an explicit scheduler slot.

## Cancellation model

### Observation versus authority — LOCKED 2026-09-13

The cross-context `Cancellation` object is read-only: it exposes `isRequested()` and
`throwIfRequested()`, not the authority to cancel. The current Async implementation keeps manual
cancellation authority on the owning `TaskGroup` through `cancel(?Throwable $reason = null)`.
Repeated `TaskGroup::cancel()` calls while the scope is live are idempotent no-ops after the first
request: they throw no error and cannot replace the first request's absent/original reason. Async
group mutation is serialized by its single-threaded scheduler context; Parallel group cancellation is
owned by the parent execution as specified below and makes no cross-thread group-level safety promise.

### Parallel manual cancellation authority — LOCKED 2026-09-21

`Parallel\TaskGroup::cancel(?Throwable $reason = null): void` cancels the entire group
cooperatively. Its existing private `__cancelAll()` drives the native
`elephc_parallel_job_cancel()` operation for every queued/running child. The first request wins.
Group authority is parent-scope-only: the TaskGroup object and its mutation hooks are not transferable
to workers, and no worker callback can invoke group-level cancellation. The owning parent execution
is the sole caller. Public group operations also check the native worker-context flag before reading
group state: a valid worker-local forged or dynamically reached TaskGroup fails with
`Error("Elephc Parallel TaskGroup cannot be used from a Parallel worker")` before any native job
mutation. Cross-arena PHP pointers remain rejected at transfer and are outside this guard's scope.
The parent's loop requests each job under that job's mutex, so the group operation is not
one atomic all-jobs transaction. A worker may finish while that loop is progressing; already
terminal jobs retain their outcome and every still-Running job has its request bit set when the call
returns. The parent `Cancellation` state records its request/reason before the per-job loop begins,
so immediately after `cancel()` returns the parent token reports the request and its first reason;
there is no parent PHP/user-code turn inside that synchronous loop. Workers still observe their
independent native bits according to each job's mutex linearization. A worker may publish a terminal
outcome before or after the parent call returns; that timing cannot clear or replace the parent's
requested bit/reason, which remains observable through the token until scope close.
Each job request and observation linearizes under that job's mutex. When `cancel()` returns, any job
that is still `Queued` or `Running` has been processed under the mutex: a Queued job becomes terminal
Cancelled, a Running job has its request bit set, and already terminal jobs are skipped as no-ops
(their outcomes do not change and no error is thrown).
Dequeuing from the private FIFO does not alter a job's native phase or remove its Future from the
TaskGroup's cancellation registry. Thus the parent cancellation loop can still win `Queued ->
Cancelled` after dequeue but before `mark_running()`; if `mark_running()` acquires the job mutex first,
the cancellation loop instead sees `Running` and sets its cooperative request bit. The mutex makes
these two outcomes mutually exclusive.
A Running worker may already have passed its explicit check before the request; it can still execute
until its next cancellation point or publish a terminal outcome, so there is no preemption or
guarantee about user instructions already in flight.
Jobs already terminal before their cancellation operation keep their terminal outcome. The optional
Throwable remains in the parent context only. During that parent scope, the existing
`Cancellation` token reports the request and `throwIfRequested()` yields a `CancelledException`
whose `reason()` is that same Throwable; there is no separate reason getter. Workers observe only
the native requested bit and construct a local `CancelledException` with `reason() === null`, so no
PHP pointer crosses an arena. Scope close clears the parent reason. Individual `Future` cancellation
remains outside v1. **HISTORICAL 2026-09-21:** the former group-cancel worker/reason/queued-terminal
regressions and `parallel_` 34/34 matrix belong to the pre-Tenth candidate; the current exact
matrix is recorded in the final exact-hash precondition immediately before the consensus procedure.
An already queued job becomes terminal `Cancelled` without acquiring a context. A running job stays
Running until it publishes a result/failure or exits. A published result or a genuine
non-cancellation Throwable wins that terminal publication even when cancellation was requested
earlier. An uncaught worker-local `CancelledException` observed from that request publishes no
failure envelope and becomes terminal `Cancelled` at worker exit. `Future::join()` on either
cancelled path throws a fresh parent-side
`Elephc\Async\CancelledException` whose `reason()` is `null`; cancellation is neither a
`TaskFailure` nor a `TaskGroupFailure`, and scope drain releases its native job normally.
When the root body returns normally after deliberate group cancellation and every child is terminal
Cancelled with no genuine worker failure, `Parallel\run()` returns that root value.
No worker can be admitted after a cancellation request: `spawn()` fails before job creation. For jobs
admitted before the request, a direct token argument/capture is a fresh read-only proxy over that
job's native requested bit; even when the parent request carried a Throwable reason, the proxy and
worker-side `CancelledException` expose no such reason.
Async request ordering is serialized by the single-threaded executor: an explicit
`TaskGroup::cancel()` linearizes when its synchronous call sets the request; an implicit fail-fast
request linearizes when the executor catches a task Throwable and requests cancellation before
beginning another task's drive turn. No two such request operations execute concurrently. If an
explicit request occurs before a later failure is caught, its null/original reason wins; if the failure
is caught and requests first, the failure reason wins over a later explicit call. When one task
explicitly cancels and then throws within the same Fiber activation, the explicit call is first.
For Async cancellation, the first request wins. If an escaping task Throwable initiates the first
request, that Throwable becomes the cancellation reason, which cooperative children may observe
through `CancelledException::reason()`. If an explicit `cancel(null)` or `cancel($reason)` is the
first request, a later task failure does not replace its absent or original reason. This Async reason
is observable only inside the shared Async runtime context and is never transferred to a Parallel
worker; Parallel workers always observe only the native requested bit and a null local reason,
regardless of any parent-side Async failure reason. For Parallel
fail-fast, a root Throwable remains a parent-local
drain cause while a child failure requests only the native bit and a null parent reason. The first
request is never overwritten: a later explicit `cancel(?Throwable)` cannot install a reason. The
parent has no user-code turn while a Parallel scope drains; an escaped token reaches post-scope
observation with its reason cleared. Workers always see a null reason.
Neither policy claims support for PHP's inherited
`Throwable::getPrevious()` slot, which Elephc does not currently model for compiler-owned exception
subclasses.

This preserves the locked rule that `Cancellation` is the only shared cross-arena reference while
preventing a child from cancelling unrelated siblings or its parent through a capability it only
needed to observe.

Cancellation is cooperative and advisory:

- a sleeping or I/O-waiting non-root task is awakened and receives cancellation;
- the root is never awakened by group cancellation at its own wait point; its registration remains
  active until its ordinary completion event (including terminal completion of a child it awaits);
- a never-started runnable task observes cancellation before its first activation and is terminalized
  without invoking its body; a previously started runnable task resumes at its saved safe point and
  observes the request there, allowing user catch/finally cleanup to execute;
- a running task observes it only at an explicit cancellation point in v1;
- a non-yielding CPU task cannot be forcibly stopped in v1;
- scope teardown waits for every child to finish cleanup before releasing its context.

The root is excluded from the cancellation-wake edge in the lifecycle diagram and from the cancellation
delivery rules above; its own wait registrations remain active until their ordinary completion event.

For Parallel, an active scope cannot be suspended out of its owning Fiber: direct and dynamically
resolved user `Fiber::suspend()` calls are rejected before argument evaluation while the parent
scope is active. If the Parallel root body throws, `run()` records that Throwable, requests
cancellation, drains every child, closes the TaskGroup, and only then rethrows or aggregates the
failure. Thus an exception-terminated owner Fiber cannot strand a live Parallel scope; destroying a
suspended owner while its scope is active is unreachable through the supported v1 API. In addition,
the `run()` `finally` path explicitly invokes the idempotent cancel/drain/close helper before
releasing the parent-scope guard, even if user code retained another PHP reference to the TaskGroup.
The object destructor calls the same helper as a last-reference fallback; it is not the only owner
of scope cleanup. The helper checks the already-closed case first and releases the parent guard;
`__close()` has already unset its state by then. Only an open, constructor-bypassed object with an
uninitialized typed state takes the subsequent `isset($this->state) === false` early return. Reversing
those checks leaves the parent guard active after normal closure, so their order is part of the
scope-cleanup invariant.

Async cancellation does not use `Fiber::throw()`: scheduler-owned cancellation points resume
normally and call the shared state from inside the task Fiber, preserving local catch/cleanup
semantics. Parallel cancellation must likewise never inject a Throwable pointer from the parent
arena into the worker arena. At an explicit worker cancellation point,
`Cancellation::throwIfRequested()` performs an ordinary PHP `throw` of a freshly constructed local
`CancelledException`; it never uses `Fiber::throw()` or any cross-arena exception transport.

## Failure semantics

### Async failures — LOCKED 2026-09-13

An Async task stores either its value or its Throwable in the same `_rt_ctx`. `await()` returns the
value or rethrows the Throwable. The scope still owns the task until every child has settled.

The v1 root/task-group policy is fail-fast: the first escaping child or root failure requests
cancellation of siblings, and the scope waits for every cleanup path before returning or throwing.
`Awaitable::await()` marks its target task failure as observed before rethrowing it. This observed bit
belongs to the task outcome, not to an individual waiter: once any waiter observes that failure,
scope exit excludes it even though every later waiter still receives the same Throwable from
`await()`. If the caller catches
that exception, scope exit does not throw it a second time; fail-fast cancellation already requested
by the failure is not rolled back. Every later cleanup failure remains an independent outcome, and
scope exit throws the first failure in occurrence order that was not observed through `await()`.
That sequence includes the root task and children: when a child fails first and an unobserved root
failure follows, the child Throwable is propagated; the reverse order propagates the root Throwable.
Multiple waiters may observe and receive the same task failure.
Awaiting a terminal `Cancelled` child does not set this failed-outcome observed bit: cancellation is
not a `Failed` outcome, is never promoted to a failure at scope exit, and is not part of the
unobserved-failure selection.

A task awaiting its own `Awaitable` throws
`Error("An Elephc Async task cannot await itself")` immediately. A longer wait cycle that has no
runnable task, timer, I/O registration, or cancellation wake source ends with
`Error("Elephc Async deadlock: tasks are live but none are runnable")`, rather than blocking
indefinitely in `poll()`.

Raw user `Fiber::suspend()` is not an Async wake operation and does not register an Awaitable,
timer, or I/O source. Direct calls and dynamic callables resolving to that method raise
`Error("Fiber::suspend() is not a scheduler wakeup inside an Elephc Async task")` before suspension
while an Async task is active. User tasks yield through `Awaitable::await()`,
`TaskGroup::reschedule()`, `TaskGroup::sleep()`, or descriptor waits; scheduler-owned Fiber
suspensions inside those methods remain supported. The deadlock diagnostic above remains the
fallback for live tasks with no registered wake source.

Deliberate `TaskGroup::cancel()` is a terminal cancellation request, not an unobserved task
failure. The root body may therefore return normally after it requests cancellation; `run()` drains
the cancelled children and returns that root value. Awaiting an individual cancelled child throws
`CancelledException` at the await site with the scheduler's first-request reason, including when
the waiter is the root task. A deliberate `cancel(null)` remains the first request and is not
superseded by a later cleanup failure.

The Async root task is excluded from cancellation wakeups and is not implicitly cancelled at its own
`reschedule()`, `sleep()`, or descriptor wait points. A root sleeping or waiting for I/O therefore
remains parked until its normal timer/readiness wakeup, then continues without an injected cancellation
exception. In particular, if a child requests group cancellation and then settles while the root is
parked in `awaitReadable($stream, null)` with no readiness event, cancellation does not deregister or
wake the root: `poll()` may remain blocked indefinitely, and the deadlock diagnostic does not fire
while that I/O registration is an active wake source. A finite sleep/I/O deadline or ordinary I/O
readiness wakes the root normally; after resumption it continues without injected cancellation and
must explicitly leave the scope or observe the token/child outcome. It remains responsible for leaving
the scope. The exemption is only from scheduler-injected cancellation at the root's own wait points:
an explicit root call to `Cancellation::throwIfRequested()` still throws when the group token is
requested, and `isRequested()` reports `true`; the root may catch that exception and continue. If it
awaits a child that has terminalized Cancelled, terminalizing that child is an ordinary task-completion
wake for the root's `WaitingTask` registration (not a group-cancellation wake of the root). At its
next FIFO turn, the root's resumed `await()` skips child-only automatic cancellation injection,
detects the child's Cancelled outcome, and throws the request-paired `CancelledException` in the root
Fiber. If the root
catches that object it may continue and return normally. If it escapes, the scheduler records it as
the root's ordinary failure outcome (a root task never has the child-only Cancelled terminal phase),
drains children. If this root exception is the first unobserved failure in occurrence order,
`Async\run()` rethrows the same exception object with its first-request reason intact; an earlier
unobserved child failure remains the propagated outcome under the existing occurrence-order rule.
This Async state/reason retention is distinct from Parallel's parent-local reason, which is cleared
at Parallel scope close; the Parallel cleanup rule does not clear or rewrite an escaping Async root
exception.
The root reducers pin the escaping-object identity/reason, earlier-child precedence, and continued
execution through the root's own yield points.

### Parallel failures — LOCKED 2026-09-16

A Throwable object never crosses arenas. A worker serializes a failure envelope containing the
remote class name, message, code, source location, and available compiled stack frames. The parent
constructs `Elephc\Parallel\TaskFailure` from that copied envelope. Multiple failures are retained
in deterministic submission order even if completion order differs. Submission order is the order
in which synchronous `TaskGroup::spawn()` calls create and reserve their monotonically increasing
native job IDs; worker admission order and completion order cannot reorder it.

A Rust panic must be caught inside the bridge; unwinding across the C ABI is forbidden. A caught
worker callback or worker-thread panic surfaces as `TaskFailureKind::WorkerPanic`. `Infrastructure`
is reserved for non-panic executor failures such as a callback that returns without publishing an
outcome or cleanup infrastructure failure. A fatal PHP exit, arena exhaustion, context acquisition
failure, and worker panic have distinct machine-visible failure kinds.

`Parallel\run()` drains every submitted child to a terminal phase before constructing its final
aggregate. Therefore a child that fails after the root body returned normally but before drain
completes is included if its failure remains unobserved; root return is not an outcome cutoff.
If the root closure throws while one or more child failures remain unobserved, neither side may be
discarded. With only a root failure, `run()` rethrows the original Throwable. With child failures
only, it throws `TaskGroupFailure` as already specified. With both, it throws `TaskGroupFailure`;
`rootFailure(): ?Throwable` exposes the original root failure separately and `failures(): array`
contains only unobserved child `TaskFailure` values in submission order, regardless of when the root
failure occurred. This is the order-preserving subsequence of all failed children after removing
those already observed by `join()`; when a failure is observed changes membership only, never the
relative order of remaining entries. The root failure is never inserted into or used to reorder that
child array. The inclusion predicate is exact: a child is
included iff it reached Failed and its `Future::join()` had not already observed that failure.
Cancelled outcomes are never included, while a genuine worker failure remains included even if the
group cancellation request predates its terminal publication. This preserves the ordinary root
exception type when no aggregation is necessary while making simultaneous outcomes lossless.

## Parallel executor refinement

### v1 scheduling — LOCKED 2026-09-16

The OS scheduler owns CPU fairness and preemption between sandbox threads. Elephc's Parallel
executor only needs bounded admission and work assignment:

- a task that cannot immediately acquire a context waits in submission order;
- admission is bounded, while the v1 FIFO submission queue is deliberately unbounded; ordinary
  saturation queues rather than overflows or rejects. The current implementation has seven worker
  slots after reserving one main slot, but that numeric capacity is not a public v1 guarantee;
- `ContextUnavailable` is not a queue-overflow signal: it occurs only when an admitted worker's
  configured context-acquire callback returns null;
- cancelled queued work never acquires an arena;
- one worker executes one PHP task at a time in its installed `_rt_ctx`;
- a context is released only after result/failure transfer and all worker-side cleanup;
- a worker may not enter another `Parallel\run()` scope in v1; the checker rejects direct and
  transitively known nested use, and the runtime fails closed for any path the checker cannot prove.
  A dynamic worker invocation becomes `TaskFailureKind::PhpThrowable` with remote class `Error`
  and message `Elephc\Parallel\run(): nested Parallel scopes are not supported inside a Parallel
  worker in v1`;
- a second `Parallel\run()` from the parent root is also runtime-rejected, including dynamic
  dispatch, with `Error("Elephc\Parallel\run(): nested Parallel scopes are not supported in a
  parent root in v1")`. This is the explicit v1 reentrant-executor refusal;
- no work stealing or task migration is required before measurement demonstrates a need.

A successful Parallel `TaskGroup::spawn()` returns after its payload/job is queued; it never waits for
worker admission, regardless of queue depth, and there is no public saturation/backpressure query.
An allocation failure during payload/job enqueue follows the host allocator-failure behavior described
above; v1 does not promise a recoverable queue-full exception.

The per-job mutex transition from `Queued` to `Running` is the only admission/cancellation
linearization point and occurs immediately before entering the worker runtime callback. Dequeueing a
submission or reserving/spawning its executor thread is not a job-state transition: until
`mark_running()` wins the per-job mutex, the job remains `Queued`, with no intermediate public phase.
Cancelling a job does not synchronously compact the executor's private FIFO deque: its entry remains
as a terminal placeholder until the next admission scan pops and skips it. Consecutive cancelled
entries are skipped one by one without acquiring contexts or changing the relative order of live
submissions behind them.
If cancellation changes a
job from `Queued` to terminal `Cancelled` first, the executor skips it while scanning the FIFO queue
or a concurrently launched worker observes the failed `mark_running()` transition and exits before
context acquisition or callback invocation; a briefly launched no-op thread releases its executor
slot without consuming a worker context. The queued-to-Cancelled phase write and its condition-variable
notification occur under the same job record; a `Future::join()` already waiting on that Queued job
wakes as soon as cancellation publishes the terminal phase, without waiting for the next admission
scan. If `mark_running()` wins first, the job is Running and the
callback is invoked; a later request sets its cooperative bit and user code observes it only at an
explicit worker cancellation point. A cancelled queue head never consumes a context or delays
admission of the next still-queued submission beyond the short no-op-thread teardown in that race.

Bounded admission is normative: at most the worker contexts currently available are admitted at
once, and saturation queues submissions FIFO without a queue-length cap. The numeric admission
capacity is implementation-defined; the current eight-slot context pool (one main plus seven worker
slots) is an implementation probe, not a public concurrency contract. The v1 API must not expose or
promise that number or expose a runtime capacity query; callers observe saturation only through
submission queuing. V1 provides no backpressure, queue-length cap, or cancellation-free overflow
error; queued tasks and their retained transfer payloads consume memory, and sustained submissions
without worker progress may exhaust available memory. There is no recoverable v1 queue-full contract:
allocator exhaustion may produce the host PHP fatal/allocation failure, a Rust allocation abort, or
OS process termination; it is not guaranteed to become a per-task `TaskFailure` or a specific
recoverable PHP exception. `Parallel\Runtime` and persistent configurable
worker pools remain deferred as already decided in `sandbox-threads.md`.
FIFO guarantees order among queued jobs when an admission slot becomes available; v1 does not promise
starvation freedom or completion deadlines. A worker callback that never returns can occupy a bounded
slot indefinitely, leaving later submissions queued indefinitely even when their order is preserved.

### Parallel public API — LOCKED 2026-09-16

Mirror the structured shape without merging the result types:

```php
namespace Elephc\Parallel;

function run(Closure $body): mixed;

final class TaskGroup
{
    public function spawn(Closure $task, mixed ...$args): Future;
    public function cancellation(): \Elephc\Async\Cancellation;
    public function cancel(?\Throwable $reason = null): void;
}

final class Future
{
    public function join(): mixed; // throws CancelledException for a cancelled job
    public function isComplete(): bool;
}

final class TaskFailure extends \RuntimeException
{
    public function kind(): TaskFailureKind;
    public function remoteClass(): ?string;
    public function remoteFile(): ?string;
    public function remoteLine(): int;
    public function remoteFrames(): array; // ordered [function, file|null, line] entries
}

final class TaskGroupFailure extends \RuntimeException
{
    public function failures(): array; // ordered TaskFailure entries
    public function rootFailure(): ?\Throwable;
}
```

The Parallel PHP declarations are demand-injected: a statically visible
`Elephc\Parallel\run()` call injects the prelude and its Async cancellation dependency. Before
name resolution, the detector conservatively matches function references whose final segment is
`run`, so unrelated same-named references may also inject the prelude; no exact-absence guarantee
is attached to that heuristic. `--with-parallel` force-links the native worker bridge only: the
flag itself does not inject PHP declarations or enable an opaque dynamic Parallel entry point.
This is an explicit exception to ordinary prelude-bridge force-injection behavior. The CLI fixture
with no `run` reference proves that `class_exists("Elephc\\Parallel\\Future")` remains false even
when the bridge is force-linked.

`TaskGroupFailure` uses the stable generic message `One or more Elephc Parallel tasks failed` and
code `0`; the child-specific messages remain on each ordered `TaskFailure`. The optional root failure
is exposed only through `rootFailure()` and is not inserted into `failures()`.

Each `spawn()` creates one non-cloneable `Future` object and that object exclusively owns its copied
terminal bytes or parent-side `TaskFailure` after scope drain; its destructor releases that owned
state exactly once. Ordinary PHP aliases to the same Future object are allowed and do not create
another owner. `clone $future` is rejected by its inaccessible private `__clone()` before a second
handle can acquire the same native job or copied state. `Parallel\TaskGroup` is likewise non-cloneable
to prevent copied scope authority/cleanup state; aliases to the same object remain the same capability.

`Future` is an opaque compiler/runtime-constructed handle. User code cannot construct it from a
native job ID or invoke compiler/runtime bookkeeping hooks on `TaskGroup`; those capabilities are
private PHP methods reached only through narrow compiler-owned friend channels. `TaskGroup` also
has a private constructor; `Parallel\run()` is its only v1 creator. Direct `new`, including a
dynamically resolved constructor call, therefore fails through PHP's ordinary private-constructor
visibility `Error` before a group is created. Reflection APIs that bypass constructors are not a
supported scope-entry mechanism and cannot manufacture the private native scope authority; an
uninitialized forged object is not a usable TaskGroup. The destructor is public as PHP requires for
engine-driven destruction; an explicit userland call to `TaskGroup::__destruct()` runs the same
idempotent cancel/drain/close helper immediately. It ends that group early, after which operations
throw `Error("Elephc Parallel TaskGroup cannot be used after its Parallel\\run() scope has closed")`;
ordinary structured cleanup still runs the same helper from `Parallel\\run()`'s `finally`. A Future may leave
its dynamic `Parallel\run()` scope after that scope has joined every worker. The TaskGroup records
the actual handle objects, settles them during drain, copies successful wire bytes or a failure
envelope into parent/native-owned terminal state, and drops its owners. An escaped handle retains
only that copied terminal state — never a worker context or arena pointer. Scope drain releases the
native job for every registered Future and clears its job ID; the escaped destructor then releases
only copied result/failure storage (with a defensive native-release fallback only for an impossible
undrained handle). A successful post-scope `join()` decodes fresh PHP values; a failed handle
retains and rethrows its parent-side `TaskFailure`, including after the scope's
`TaskGroupFailure` has been caught.

The root body may call `Future::join()` before scope exit. It may block until that one job settles,
including while the job is still `Queued` and waiting for an admission slot; joining does not promote
the job, reserve a worker context, or change its FIFO admission order. It may therefore wait
indefinitely behind earlier work that does not complete.
The TaskGroup continues to own and track the handle through the all-worker drain. A successful
joined value is not aggregated at scope exit; a failed outcome already observed by `join()` is
excluded from `TaskGroupFailure`, while every other unobserved failed child is retained in submission
order. `join()` marks a failed job observed inside the call before it throws the `TaskFailure`,
independent of whether the caller catches that exception. If it escapes the root body, it is the root
failure; that child is still excluded from the child-failure array, so the same failure is not
aggregated twice at that scope exit. There is no Future callback API in v1.

If an unobserved child failure was already included in a materialized `TaskGroupFailure`, a later
post-scope `Future::join()` still rethrows that Future's retained `TaskFailure` and may mark the native
failure observed for bookkeeping, but it cannot mutate the already-thrown aggregate snapshot. The
two observations are intentional and independent.

`Future::isComplete()` is non-blocking and returns `true` for every terminal phase: successful,
failed, and cancelled. It does not observe or consume a failure. `TaskFailure::remoteLine()` returns
the integer source line where the envelope has a location and the sentinel `0` for location-less
kinds; `remoteFile()` and `remoteClass()` are then `null`.

`Future::isComplete()` reflects the job's mutex-protected phase, not its request bit. A queued-job
cancellation changes `Queued` directly to terminal `Cancelled` under that mutex before waking joiners,
so subsequent `isComplete()` returns `true`; a running-job request alone leaves phase `Running`, so
`isComplete()` remains `false` until result/failure/cancelled terminal publication.

`TaskGroup` itself is a non-escapable scope capability. The checker rejects a `Parallel\run()`
body whose statically proven return type contains its root group, including a result declared
`mixed` when the body return is still proven to be `TaskGroup`. This does not apply to `Future`.
The runtime independently marks a group closed after scope drain. Its public `spawn()`,
`cancellation()`, and `cancel()` operations, and the compiler-only lowered `spawn()` path, all
assert that state after ordinary receiver/callable/argument evaluation but before payload or job
creation. A dynamically erased path that reaches a closed group therefore throws
`Error("Elephc Parallel TaskGroup cannot be used after its Parallel\\run() scope has closed")`;
it cannot start work after the owning scope has ended. This is identical whether closure came from
`Parallel\\run()`'s `finally` path or the destructor's idempotent fallback.
“Non-escapable” means its live authority cannot outlast the structured scope, not that userland
cannot retain an ordinary PHP object alias by side effect. `run()` closes that object in `finally`
before returning or throwing, so every later operation fails closed.

As for Async, the closure passed to `Parallel\run()` must accept its root `TaskGroup`; the checker
rejects an incompatible signature.

`Elephc\Async\run()` is explicitly unsupported inside a Parallel worker in v1. The shared
transitive callable-safety analysis rejects a worker task that directly or indirectly enters an
Async root scope with a named checker diagnostic. Parallel injection also prepends a runtime
worker-context guard to the injected `Async\run()` entry: runtime-resolved calls, including
`call_user_func("Elephc\\Async\\run", ...)`, throw `Error` before a scheduler state is allocated.
This prevents a worker-local reactor/scheduler from emerging accidentally; deliberate cross-domain
composition remains deferred to the explicit adapter design.

`Async\run()` is nevertheless supported inside the parent root body of `Parallel\run()` when that
body was entered from the main execution context with no Async scheduler active. It blocks
synchronously until its nested Async scope returns. If the Parallel root is called from an Async
task, its parent body is still in that task's Fiber and a further `Async\run()` is rejected by the
ordinary any-Fiber nesting guard with its exact `Error`; the composition reducer pins that outcome.
The worker-only guard is separate and continues to reject any Async root in an isolated worker.

`Future::join()` blocks the calling OS thread in v1. Calling it from an Async task therefore blocks
that Async scheduler and produces a compile-time warning when the checker can prove the call
context. A later explicit `Async\awaitFuture(Future)` adapter may suspend the Fiber and wake
it on worker completion; no implicit `Future` to `Awaitable` conversion is planned.
This blocking call is not interruptible by Async cancellation: no other task in that `_rt_ctx` runs
while the OS thread is inside `join()`. A cancellation or failure already recorded before the call
remains pending; the current Fiber remains Running while blocked, and no cancellation exception can
be delivered inside `join()`. `join()` returning a value or throwing a `TaskFailure` is not itself a
cancellation point: if user code catches a `TaskFailure`, it continues synchronously until the next
explicit cancellation point; if it does not catch it, the failure escapes by ordinary task-failure
semantics. For a non-root task, the pending request is delivered only at a later explicit cancellation
check: `Cancellation::throwIfRequested()`, or the post-resumption check of a suspension-based
`await()`, `reschedule()`, `sleep()`, or descriptor wait. An `await()` of an already-terminal task
that does not suspend is not such a check.
The root remains exempt from implicit cancellation at its own points. The scheduler can then resume
queued work and drain it. If join does not return, the enclosing `run()` remains blocked and
cannot advance cleanup. No task can concurrently originate a new Async failure or cancellation on
that same `_rt_ctx` during the blocking call, so a request cannot newly arise from a sibling Async
task while the parent Fiber is blocked inside `Parallel\\run()`/`join()`. A request already recorded
before entry is deferred, not lost, and is checked only at the later explicit point described above.
This is the documented consequence of a blocking
operation, not concurrent scope abandonment or an implicit cancellation guarantee.
Timers keep their absolute monotonic deadlines and Async descriptor registrations retain their
duplicated descriptors while `join()` blocks; the scheduler does not poll or deregister them during
that call. Once it returns, the Async drive loop resumes and processes expired deadlines and current
readiness in the normal order. This is intentional blocking/starvation behavior, not transfer of
reactor ownership.
There is no deadlock-free or bounded-progress guarantee for this operation: if the worker or other
synchronous user code it depends on does not return, `join()`, the containing `Parallel\run()`, and
any surrounding Async scope remain blocked. The warning is diagnostic, not a guarantee of progress.

## Reactor and I/O contract

### One reactor owner — LOCKED 2026-09-13

One Async scheduler owns one reactor. A process must not accidentally nest a custom Fiber loop
inside Tokio, or Tokio inside a Fiber loop, and expect both to make progress. `elephc-web` eventually
needs one authoritative worker loop with web connections and Async task wakeups as event sources.

The reactor boundary should be backend-neutral:

```text
register(task_id, source, interest)
update(task_id, source, interest)
deregister(task_id, source)
poll(next_deadline) -> ordered wake events
wake()
```

Existing evidence narrows the choice:

- the Fiber HTTP showcase already links libc's `poll()` on the supported POSIX targets and uses
  the common eight-byte `pollfd` layout;
- `stream_select()` is not a suitable reactor substrate because its emitted helper mutates PHP
  arrays and currently represents only descriptors `0..63`;
- importing Tokio, mio, or another executor would create a second ownership model before the
  single Elephc scheduler/reactor boundary is stable.

The locked v1 backend is a small dependency-free, level-triggered `poll()` adapter
called through the platform C ABI. It returns registration IDs and readiness flags in registration
order; the PHP/AST scheduler retains task IDs, append-only registration IDs, deadlines, and
ownership. The
backend-neutral boundary remains authoritative so Linux may later move to epoll and Darwin/iOS to
kqueue without changing the PHP API.

### Descriptor-wait surface — LOCKED AND IMPLEMENTED 2026-09-13

```php
final class TaskGroup
{
    public function awaitReadable(mixed $stream, ?float $timeout = null): bool;
    public function awaitWritable(mixed $stream, ?float $timeout = null): bool;
}
```

`mixed` is required because PHP has no legal `resource` parameter type. The checker/runtime still
reject values that are not selectable native stream resources with `TypeError`. A raw integer
descriptor must fit `0..=2147483647`; negative or larger values raise `ValueError` before duplication
or registration, with message `TaskGroup::awaitReadable() expects a descriptor within the non-negative
C int range` (or the corresponding `awaitWritable()` name). `null` means no deadline. A timeout must
be finite, non-negative, and small enough
that adding its nanoseconds to the current monotonic clock fits a signed 64-bit deadline; NaN,
infinity, negative, and out-of-range values raise `ValueError` before descriptor duplication or
registration. Zero performs a non-blocking probe, and expiration returns `false`. Readiness, hangup,
or an OS error returns `true` so the following stream operation observes EOF or its native error.
Cancellation throws `CancelledException`.

The scheduler retains a PHP stream resource as a resource until the runtime adapter extracts its
native payload and duplicates the underlying descriptor. Casting it to `int` is incorrect: Elephc
exposes the PHP resource identifier, which need not equal the OS descriptor. Closing the original
PHP stream after registration does not close or invalidate the scheduler's duplicate; it remains
registered and is owned/closed only by the scheduler when its registration is released. User code
does not receive the duplicate descriptor and cannot close it through the original stream handle. The
registration also retains its PHP resource zval until that registration is released, so dropping the
caller's final reference alone cannot run the resource destructor early; explicit `fclose()` closes
the original handle while the independent duplicate remains valid. Only native selectable streams
are accepted here; user-wrapper `stream_cast()` state is not part of this contract.
`poll()` reports readiness, hangup, or an OS error on that duplicate as `true`; an invalid externally
closed duplicate yields the platform's poll error/hangup result (`true`), not an indefinite wait on an
invalid `pollfd`. If the underlying object remains valid but never becomes ready and no timeout was
supplied, the wait may remain pending. Any integer
within `0..=2147483647` is accepted as a raw
descriptor; the runtime cannot prove FFI/native provenance, so the caller is responsible for
passing an open, selectable OS descriptor rather than a PHP resource ID. Passing `(int)$stream` or
another PHP resource ID as an integer is a caller error: it may refer to the wrong descriptor or an
invalid descriptor, and the runtime does not validate provenance or open/selectable status. A
negative or out-of-range integer is rejected before
it reaches `poll()`: POSIX ignores negative `pollfd.fd` entries, which could otherwise leave a
null-timeout wait blocked indefinitely while the scheduler believes it has an active I/O wake
source.

Repeated registrations of the same PHP stream are allowed, including simultaneous read and write
waits or waits owned by different tasks. Each registration independently duplicates the descriptor,
owns that duplicate until its own readiness, timeout, cancellation, or scope teardown, and releases
only its own duplicate. Poll readiness for a shared underlying open-file description may wake each
matching registration; task-generation and per-registration active checks make duplicate delivery
idempotent without transferring ownership between waiters.

These methods wait only for readiness: they neither perform I/O nor silently change descriptor
blocking flags. Callers must put the stream in non-blocking mode before using operations that could
otherwise block after a partial read/write. User-wrapper `stream_cast()` callbacks, regular-file
offload, DNS, TLS, curl, and databases remain outside this first descriptor slice.

The scheduler performs a zero-time reactor poll before each task dispatch when registrations exist,
then blocks in `poll()` only when the ready queue is empty. Existing ready work stays ahead of newly
delivered I/O events; events from one poll append in registration order. The blocking timeout is the
minimum of timer and descriptor deadlines and is recomputed after EINTR.

### Blocking honesty — LOCKED FOR THE DESIGN

A Fiber does not make a blocking syscall asynchronous. Existing PHP builtins retain PHP behavior;
the scheduler must not silently change `sleep()`, `fread()`, PDO, curl, or arbitrary FFI calls.
Async-aware APIs use non-blocking descriptors, bridge-native asynchronous operations, or an
explicit blocking-offload facility. Every unsupported blocking boundary remains visibly blocking.

The first useful I/O slice should be timers plus readable/writable descriptor readiness. Database,
curl, DNS, TLS, regular-file I/O, and user callbacks require separate designs and must not be claimed
by a generic Async label.

## Web integration boundary

Async inside one PHP request and concurrent PHP request handlers are different features.

Intra-request Async can share the request's `_rt_ctx` and request/response state. It can overlap
outbound waits performed by tasks belonging to that request.

Inter-request concurrency in one web worker is not safe merely because Fibers exist. The web bridge
currently relies on process-static request/response storage and on one handler at a time. Supporting
multiple interleaved handlers requires either:

1. request state made task/context-local and a safe way to switch `_rt_ctx` between runnable request
   Fibers on one OS thread; or
2. isolated Parallel contexts/threads for handlers, with explicit request/result transfer.

This decision is DEFERRED. The Async v1 must not claim that `--web` can concurrently execute several
PHP request handlers inside one worker.

## Monitoring model

Scheduler observability should reuse the existing monitoring vocabulary and dormant-path rules.
Each task event should carry:

- task ID, parent ID, task-group ID, and execution domain;
- runtime-context and worker identity where applicable;
- state transition and wake reason;
- runnable-queue delay, on-CPU duration, blocked duration, and cancellation latency;
- I/O category and trace context when the underlying operation already declares them.

Scheduler bookkeeping is infrastructure, while the PHP operation remains the visible work boundary.
Dormant monitoring must not allocate per scheduling event. FIFO v1 does not require a clock read for
every activation; timing work is enabled only when monitoring or a timer requires it.

Within Async, existing trace/monitor context follows the Fiber task. V1 Parallel exposes task,
parent, group, runtime-context, and worker identity to monitoring, but does not currently capture or
propagate PHP trace/span IDs. Cross-domain trace correlation is deferred. If added later, the parent
captures immutable trace metadata synchronously at `spawn()`; the transfer envelope carries copied
metadata, the worker reconstructs its local monitor context before invoking the callback, and worker
teardown releases it. No PHP trace-context object or arena pointer crosses the boundary.

## Target policy

- The scheduling semantics and public diagnostics are target-neutral.
- macOS AArch64, Linux AArch64, and Linux x86_64 need executable end-to-end coverage.
- The iOS targets are library-only and currently do not execute Fibers. **IMPLEMENTED FOR ASYNC:**
  `--check` remains available, while an AOT compile that would emit an Async artifact fails before
  producing the artifact with diagnostic `Elephc Async execution is unavailable for target
  '<target>': iOS libraries do not yet have a host-driven Fiber scheduler contract. Use --check for
  analysis, or emit for macOS/Linux.` until a host-driven Fiber scheduler contract exists. This is a
  compile-time refusal, not a runtime `Async\\run()` error.
  Parallel must follow the same fail-closed
  rule unless it gains an explicit host contract; currently Parallel `--check` succeeds, while AOT
  artifact emission with the Parallel runtime is refused before producing an artifact on both iOS
  targets with diagnostic `Elephc Parallel execution is unavailable for target '<target>': iOS
  libraries do not yet have a host-driven worker-thread contract. Use --check for analysis, or emit
  for macOS/Linux.` This is also a compile-time refusal, not a runtime error.
  The explicit tests reject artifact emission on both iOS
  device and Simulator targets. The explicit library-refusal/`--check` reducer passes **1/1** for
  each target; no silent no-op or missing symbol is acceptable.
  If one AOT program requires both runtimes, `--check` still succeeds, while artifact emission reports
  only the first applicable compile-time refusal in pipeline order: Parallel's worker-thread
  diagnostic precedes Async's Fiber diagnostic, and compilation exits before artifact creation. The
  messages are not combined; each single-feature message above is stable and asserted by the target
  tests.
- This is target-matrix conformance through explicit diagnostics, tests, and isolation—not a claim
  that iOS executes the Fiber/worker runtime today. Executable-host readiness is claimed only for
  macOS AArch64, Linux AArch64, and Linux x86_64. The named Async **54/54** and Parallel **58/58**
  suites are the end-to-end acceptance matrix for the implemented Candidate-35 runtime baseline on
  those hosts; they do not validate the later design-only additions listed in the current precondition.
  Separately labeled
  macOS-only C-host fault-injection probes are supplemental diagnostic evidence, not Linux coverage
  and not proof of target-matrix closure for the resource-table failure paths they isolate; those
  specific diagnostic paths remain an explicit cross-target validation follow-up where Linux runs
  are absent from the ledger.
- Every context-switch, safe-point, callback, and foreign-entry rule remains symmetric across the
  AArch64 and x86_64 backends.

## Implementation progress

### 2026-09-13 — Async kernel slice 1 complete

Implemented as a compiler-injected elephc-PHP prelude over the native Fiber runtime:

- `Async\run()` owns and drives the root scope;
- `Async\run()` rejects nested entry and entry from an unrelated Fiber, avoiding a second event
  loop without process-global scheduler state;
- `TaskGroup::spawn()` is the only public task-creation operation;
- FIFO scheduling is deterministic and never runs a spawned task inline;
- `Awaitable::await()` suspends on an unfinished task and is requeued when it completes;
- terminal failures are stored per task, so each `Awaitable` reports its own Throwable and
  multiple waiters may observe the same task outcome;
- `TaskGroup::reschedule()` voluntarily moves the current task to the FIFO tail without
  process-global scheduler state;
- `TaskGroup::sleep(float)` parks only the current task on a monotonic deadline and blocks the
  thread only when the ready queue is empty;
- `TaskGroup::awaitReadable()` and `awaitWritable()` register native stream resources or raw
  descriptors with the libc `poll()` reactor, sharing monotonic deadlines with timers;
- descriptor registrations are append-only within one scope, readiness preserves registration
  order, and inactive registrations release their retained stream resource;
- the full runtime assembles for macOS AArch64, Linux AArch64, and Linux x86_64, while the Async
  example passes explicit `--check` on both iOS target variants;
- `Async\TaskGroup::cancel()` owns cancellation authority while `Cancellation` remains a read-only
  token; queued children are skipped and parked children wake at their cancellation point;
- arguments and scalar, string, and array results stay in the owning runtime context;
- root completion waits for every child, and a child failure cannot be dropped silently;
- failures are fail-fast: siblings are cancelled, cleanup is drained, observed failures are not
  duplicated at scope exit, and the first remaining unobserved failure is propagated;
- `--strict-php` omits the extension prelude;
- focused unit, contract, codegen, generator, and documentation audits cover the slice.

Not implemented by this slice: blocking-I/O offload, Parallel execution, or cross-domain waiting.

### 2026-09-15 — lifecycle and Fiber ownership closure (macOS AArch64 verified)

The previously retained cancellation, child-failure, reactor-registration, root-failure, and
awaited-failure graphs are now clean under heap debug on macOS AArch64. The fixes cover the whole
ownership chain rather than only the original reducers:

- every EIR exception handler snapshots the active cleanup chain, including a caller-owned
  activation record;
- direct local throws transfer their owned value and clear the source slot;
- Async failure paths stabilize owning values in cleanup-enabled frame locals;
- dynamic Fiber argument containers are parked in the Fiber descriptor and released on normal,
  exceptional, terminal, and suspended-destruction paths;
- `Fiber::start`, `Fiber::resume`, and `Fiber::throw` transfer only owning temporary receivers into
  the native switch ABI and release them exactly once on return, escape, and state error;
- converted `stream_socket_pair()` arrays transfer their ownership into the returned `mixed` cell.

Focused evidence is 31 passing Async functional tests and 55 passing Async heap/GC tests on macOS
AArch64. The three Fiber receiver-transfer reducers for `throw()` pass, as do the existing direct
statement/expression rethrow regressions. The shared builtin contract exporter builds and all three
generated-doc audits pass after regenerating 1,077 builtin records and 2,052 pages.

This closes the known local lifecycle defects. Task handles now encode a slot plus a monotonically
increasing generation; resolution rejects a stale generation, and a focused reducer proves that a
slot-zero/generation-one handle cannot bind to the same slot at generation two. Reactor registrations
remain append-only and the descriptor-reuse reducer remains green. The executable Linux acceptance
ledger was subsequently closed by the cross-target audit below.

### Fiber call-receiver ownership ABI — implemented and cross-target verified

The EIR/backend/runtime change is implemented atomically across `Fiber::start`, `Fiber::resume`,
and `Fiber::throw` for the AArch64 and x86_64 emitters. Lowering distinguishes owning temporaries
from local or borrowed receivers; the native helpers preserve returned values and escaped
Throwables while releasing the transferred receiver on normal return, propagated child exception,
and state-validation error. macOS AArch64 heap-debug reducers are green, and the focused scheduler
slices pass on Linux AArch64 and Linux x86_64.

### 2026-09-15 — Async scheduler monitoring implemented (macOS AArch64 verified)

The shared monitoring contract now owns stable execution-domain, task-state, and wake-reason codes.
The Async prelude publishes generation-checked task IDs, parent/root-group lineage, and transitions
through one internal typed runtime operation. Its generated wrapper reads a nullable profiler slot
and the active-capture word before tail-calling the consumer, so a normal or dormant binary performs
no scheduler-event allocation and no clock read.

The exact profiler attributes elapsed time to the previous `Runnable`, `Running`, or blocked state;
records cancellation latency, runtime-context and worker identity, wake reasons, and the active trace
identity; and renders the result through local and remote exact-monitor paths. Sequential
`Async\run()` calls can reuse their task tokens in one runtime context, so each created root receives
a capture-local monotonic scope ID and child events resolve against that scope instead of being
merged across runs.

Focused evidence includes six monitoring-contract tests, three scheduler-profiler accounting tests,
the macOS/Linux AArch64 and Linux x86_64 assembly guard test, the scheduler-table renderer test, and
an end-to-end CLI test that compiles and monitors a real Async program. A manual monitor run reports
eight tasks in three distinct scopes. The complete local Async functional (32 tests) and heap/GC
(55 tests) slices remain green with dormant monitoring. The Linux AArch64 and Linux x86_64
executable scheduler slices, including the end-to-end monitor test, are green in the audit below.

Async target behavior is also explicit on both iOS variants: analysis with `--check` succeeds, while
static-library emission fails with an actionable diagnostic explaining that no host-driven Fiber
scheduler contract exists. A focused CLI regression covers device and Simulator targets.

### 2026-09-15 — Linux execution closure and Mixed Fiber switch ABI repair

Docker Desktop was restarted after its backend exhausted file descriptors. The services process
dropped from approximately 245,778 open descriptors to 143, and `docker info` again answered in
three seconds. The worktree's `vendor/mysql-28.0.0` entry was also restored from Git: it had been
replaced by an absolute symlink into a deleted temporary directory, which could not resolve through
the `/app` Docker bind mount.

The first Linux x86_64 scheduler run then exposed 28 failures out of 88 tests. Every switching path
converged on a Fiber receiver becoming `null` after suspension. Generic method dispatch on a Fiber
unboxed from `mixed` called the native `start`/`resume`/`throw` wrappers without initializing their
private `receiver_owned` ABI argument. On x86_64 the stale third argument could therefore appear
true and release a Fiber still owned by its boxed Mixed cell. Mixed Fiber switching now uses the
same typed lowering as statically known Fiber receivers, stages visible start/resume/throw arguments,
and explicitly passes a borrowed receiver flag on both AArch64 and x86_64. A regression forces a
`public mixed $fiber` through start, suspend, resume, and getReturn.

Focused executable evidence after the repair:

- Linux x86_64: 88/88 `codegen_tests async_scheduler` and 2/2 `error_tests async_scheduler` pass;
- Linux AArch64: 88/88 `codegen_tests async_scheduler` and 2/2 `error_tests async_scheduler` pass;
- macOS AArch64: the new Mixed Fiber regression and the original suspended-await reducer pass.

The 28/88 count is the pre-fix run, not a changed or filtered test suite. The typed Mixed-Fiber
receiver-lowering repair above was applied without removing tests; the same 88-test Linux scheduler
selection then passed 88/88 on x86_64 and AArch64, with the two Async diagnostic tests passing 2/2 on
each target. The failed and repaired runs are sequential evidence for one test selection, not
conflicting acceptance counts.

The Linux runs use one Cargo build job, architecture-specific persistent target volumes, and the
host Cargo registry mounted read-only in offline mode. This avoids both the previous OOM risk and
the transient crates.io SSL/time-out failures observed after the Docker restart. The only emitted
warnings are pre-existing musl deprecations for `libc::time_t` and `libc::suseconds_t` in Magician
and Probe.

## Implementation slices

1. **Specification tests first:** parser/checker fixtures for the proposed API, transfer errors,
   target diagnostics, and structured ownership. No runtime execution.
2. **Async kernel:** **IMPLEMENTED (macOS AArch64, Linux AArch64, and Linux x86_64 verified)** — FIFO ready queue, root scope,
   `spawn`, `await`, explicit `TaskGroup::reschedule()`, completion/failure propagation, and
   generation-checked task handles are implemented. Supported-target acceptance remains tracked
   separately below.
3. **Timers:** **IMPLEMENTED (linear scan v1)** — monotonic deadlines, FIFO wakeups, zero-duration
   rescheduling, and blocking only until the earliest deadline. A timer heap remains coupled to the
   reactor slice.
4. **Cancellation:** **IMPLEMENTED (cooperative v1)** — observation-only token, group-owned
   authority, queued-task suppression, parked-task wakeup, cleanup catches, and cancelled-await
   propagation. Root and child failures request sibling cancellation and drain cleanup before
   propagating the first unobserved failure.
5. **Descriptor reactor:** **IMPLEMENTED (portable poll v1)** — readable/writable waits, monotonic
   timeouts, zero-time probes, EINTR retry through the scheduler loop, deterministic registration
   order, cancellation, descriptors above 63, and an equivalent socket interleaving reducer. The
   Fiber HTTP showcase migration remains a broader acceptance follow-up.
6. **Monitoring:** **IMPLEMENTED (macOS AArch64, Linux AArch64, and Linux x86_64 verified)** — typed task lineage and transitions,
   runnable/running/blocked accounting, wake reasons, cancellation latency, runtime-context/worker
   identity, trace identity, capture-local scope separation, and dormant-path gating.
7. **Parallel transfer/checker:** **IMPLEMENTED** — the checker
   recursively accepts scalars and statically proven arrays, retains safe return provenance across
   callable aliases, and permits Async `Cancellation` only as a direct task argument or direct
   closure capture. It rejects tokens nested in arrays or unions and all token task results before
   serialization. It rejects ordinary
   by-reference captures, objects (including indirect array members and receiver-bound callables),
   resources, pointers, buffers, unresolved `mixed`, callables, iterables, packed objects, and
   non-transferable return paths. Evidence: three classifier unit tests, three positive codegen
   fixtures, and focused diagnostics. The real `Elephc\Parallel` AST prelude now drives this
   checker contract and the runtime path. Its fixed-point callable summary rejects a direct or
   transitive `Elephc\Async\run()` entry from a worker task, while the conditionally injected
   `Async\run()` worker guard rejects runtime-resolved dispatch before scheduler allocation. Together
   they ensure v1 cannot create a cooperative root scheduler inside an isolated worker context.
8. **Parallel execution:** **IMPLEMENTED FOR THE LOCKED NON-VARIADIC V1 SURFACE** — the `elephc-parallel` staticlib bridge now owns a
   versioned native transfer blob whose bytes borrow no PHP arena, can move by ownership to another
   thread, validates its envelope before exposing a payload, clears outputs on failure, and prevents
   Rust panics from crossing its C ABI. `--with-parallel` links and runs end to end; bridge catalogs,
   CI/nightly build lists, nextest archives, release/Homebrew packaging, manifests, and Linux scripts
   are synchronized. The checker refuses reference-aliased arrays (including a PHP-confirmed cyclic
   construction) before serialization while still copying aliased scalars by value. Worker entry,
   callable/capture reconstruction, PHP-side value lowering/decoding, and cancellation wiring are
   implemented below.

   The worker-context mechanism beneath that API is now implemented independently of the executor
   policy: `elephc-parallel-contract` owns the heap-scaled side-table formulas and stack bounds;
   `WorkerStorage` allocates aligned arena/object/Buffer storage from that shared contract; and
   `elephc_parallel_run_worker_v1` acquires, enters, and releases one generated runtime context via
   passed function addresses. The generated `__rt_parallel_worker_entry` saves the host context
   register, publishes the worker context, derives both stack floors from the exact configured
   pthread/Rust stack size (never the main thread's `RLIMIT_STACK`), runs the callback, and restores
   the host register. A real configured pthread C-host probe plus cross-ABI assembly tests pass.

   Native Future/job state is also executor-neutral and implemented: monotonic non-reused IDs map
   to a mutex/condition-variable record with `Queued`, `Running`, `Completed`, `Failed`, and
   `Cancelled` phases. Queued cancellation becomes terminal without acquiring a context; running
   cancellation sets a cooperative observation flag. Input ownership remains native while live,
   terminal result/failure access returns a fresh blob copy so multiple joins cannot race a release,
   and job release is refused before a terminal phase.

   The failure boundary is now versioned and data-only. Its native envelope carries the locked
   failure kind, nullable remote class, message, integer code, nullable file, line, and ordered
   compiled frames; strict length validation rejects truncation, trailing bytes, unknown kinds,
   invalid optional-field encodings, and invalid C pointer/length pairs. Parent-side C views borrow
   only from the owned native blob. Failed jobs accept only this envelope and derive their kind from
   it, eliminating the former split kind/payload source of truth. Every job preallocates a small
   `ArenaExhausted` envelope so a failure while allocating the real diagnostic cannot strand the
   Future forever in `Running`.

   The v1 executor now owns bounded FIFO admission and platform-thread lifetime. Its current runtime
   pool contains eight total context slots, one reserved for the main context, leaving seven worker
   slots as an implementation capacity rather than a public concurrency promise. Only admitted jobs
   receive an OS thread; queued jobs remain threadless, cancelled
   queued jobs are skipped, and releasing one active slot admits the next submitted job. Each thread
   uses the exact configured stack size, executes one job, and is joined by `job_wait` or job release;
   concurrent waiters share one join result. A callback that returns without publishing an outcome
   becomes an `Infrastructure` failure instead of leaving a Future blocked. Nineteen bridge tests
   and three shared-contract tests pass on macOS AArch64, Linux x86_64, and Linux AArch64. The real
   configured pthread C-host context probe remains green.

   Transferable PHP values now have their own `EPV1` format instead of exposing PHP's general
   object/reference wire contract across the isolation boundary. The native writer enforces one complete
   root value and supports null, booleans, integers, floats, byte strings, nested indexed/associative
   arrays, and the shared cancellation-token ID. Validation rejects malformed tags, truncation,
  trailing bytes, zero token IDs, overfull/incomplete writers, overflow, and nesting beyond the
  fixed depth bound. Jobs now require `EPV1` for input and successful results, require `EPFL1` for
  failures, and provide workers an owned input copy for decoding. Twenty-three bridge tests pass on
  macOS AArch64, Linux x86_64, and Linux AArch64. V1 still uses PHP `serialize()`/`unserialize()` at
  each context-local boundary; `php_wire` parses the safe PHP-wire subset into an `EPV1` native
  envelope for cross-thread storage and reconstructs PHP wire bytes on the receiving context. EPV1
  is the cross-context transport representation, not a replacement for PHP's local codec.

  The existing PHP serializer can now safely participate in parent/worker codec lowering:
   its value/object counters live in `_rt_ctx`, and each active worker owns separate 65,536-entry
   object-pointer and reference-index tables through the same external side-table model as object
   handles and Buffers. Main-context
   initialization points at the legacy process-owned tables, while every serializer access in ctx mode
   is mechanically proven to route through the reserved context register.

   The PHP unserializer is isolated too: depth, nesting, allow-list policy/owner, reentrant snapshot
   link, and logical registry count are ctx scalars, while each worker owns a separate 65,536-entry
   value/reference registry. `__rt_ctx_acquire` therefore takes eight storage arguments on both ABIs;
   x86_64 validates both stack-passed arguments before atomically claiming a slot. All 43 former
   x86_64 RIP-relative accesses now use target-aware symbol helpers, including the Fiber suspension
   guard. This audit also fixed generic immediate-store and decrement helpers that bypassed ctx
   routing on x86_64. Scalar round-trip, reentrant wakeup/allow-list restoration, object
   back-reference, and exact-symbol routing pass on macOS AArch64. The eight-argument pthread C-host
   pool/worker probe passes on macOS AArch64, Linux x86_64, and Linux AArch64.

   The PHP/EPV boundary now has a strict bidirectional codec. It accepts only null, booleans,
   integers, floats, byte strings, and recursively safe PHP arrays; objects, custom payloads,
   references, malformed/trailing bytes, and EPV cancellation capabilities fail closed. PHP-oriented
   job helpers own conversion at create/input/complete/result boundaries, and a replaceable
   thread-local prepared-blob slot exposes exact pointer/length pairs to generated PHP without a
   process-global output cell. The thirty-one macOS AArch64 bridge tests are lower-level evidence;
   they predate the executable worker path. The current scheduler acceptance claim is instead the
   focused executable matrix recorded below, which replays the public Parallel path on macOS
   AArch64, Linux x86_64, and Linux AArch64.

   The locked public surface is now generated as compiler-owned AST from a retained PHP parse-parity
   oracle: `TaskFailureKind`, `TaskFailure`, `TaskGroupFailure`, `TaskGroup`, `Future`, and `run()`.
   This added the previously missing synthetic-enum builder/transcriber support instead of weakening
   `TaskFailureKind` into constants. Injection is hidden by strict-PHP and all three focused prelude
   parity/surface tests pass. The initial fail-closed placeholders have since been replaced by the
   execution path below; no synchronous fallback was introduced.

   **Execution path implemented on macOS AArch64:** `TaskGroup::spawn()` now lowers to a typed
   `ParallelSpawn` EIR operation. The parent evaluates arguments/captures in source order, stores
   transferable values in one flat PHP payload, serializes it locally, and submits a generated
   per-task worker callback with the context acquire/release/entry addresses. Literal closures and
   statically known user, builtin, and extern function closures share this path; receiver-bound,
   static-method, and variadic task closures fail closed with focused diagnostics. The worker
   reconstructs values in its own context, invokes the compiled closure directly, and publishes an
   EPV1 result or EPFL1 failure before context release. `Future::join()` caches repeatable results,
   `isComplete()` reads native phase, and scope exit owns every job until release.

   Parent-side failure reconstruction now preserves kind, remote class, binary message, code,
   file, line, and available compiled trace frames without a Throwable pointer. A process-wide
   completion generation plus condition variable lets `TaskGroup` wait for any child transition
   without polling; the first observed child/root failure requests the local cancellation token,
   cancels queued/running siblings, drains every terminal job, and reports every unobserved Failed
   child in submission order (never a Cancelled terminal outcome). Focused evidence is 34/34 bridge tests, 4 macOS end-to-end Parallel execution
   tests, 3/3 prelude tests, and the exception-constructor propagation regression.

   Two generic defects found by this path are fixed: declared container parameters now unbox a
   dynamically reconstructed Mixed container before the direct call, and specializing an inherited
   builtin exception constructor no longer retypes an unrelated constructor override merely because
   its parameter occupies the same numeric index.

   The named Async `Cancellation` token is now reconstructed worker-locally rather than
   serialized. Parallel injection alone extends the private Async state with a native job id;
   ordinary Async programs remain unchanged and do not link the Parallel bridge. Spawn removes
   Cancellation arguments/captures from the EPV payload, rebuilds them against the worker job, and
   preserves by-reference capture ABI. A running worker observes sibling cancellation through
   `isRequested()` and `throwIfRequested()` raises the concrete `CancelledException` in that worker
   context. Five macOS end-to-end Parallel execution tests, four prelude tests, nine transfer
   diagnostics, and 34 bridge tests pass.

   Failure observation is native job state: catching a `TaskFailure` from `Future::join()` marks it
   observed, so scope exit does not aggregate it again. Variadic task closures currently fail closed
   with a named checker diagnostic rather than repacking their ABI tail incorrectly; fixed-signature
   tasks still accept `spawn(...$args)`. Documentation and a runnable example cover the public API,
   transfer restrictions, blocking cost, cancellation, and failure model.

   Focused execution is green on macOS AArch64, Linux x86_64, and Linux AArch64 (eight scenarios on
   each host architecture, including user, builtin, and extern first-class function tasks). iOS
   device and Simulator accept `--check` but refuse static-library
   Parallel execution with an actionable host-driven worker-thread-contract diagnostic. A worker
   installs a context-local `setjmp` fatal boundary; process-exit paths longjmp only inside a worker,
   become `TaskFailureKind::PhpFatal`, and leave main/cdylib exit semantics unchanged. Ordinary
   worker Throwables and Rust/bridge infrastructure failures are likewise data-only and typed.
9. **Cross-domain adapter:** only after both domains are independently correct and measured.
10. **Soft preemption experiment:** only if cooperative starvation is demonstrated by a retained
   reducer and the safe-point ownership proof succeeds.

Each slice must be independently testable, preserve PHP behavior when unused, and keep the supported
target matrix explicit. Full-suite runs are not the default validation strategy; use focused slices
   and CI according to `AGENTS.md` and the recorded Kioku disk-exhaustion gotcha.

## Acceptance ledger

Unless an entry explicitly names the final exact-hash precondition before the consensus procedure,
dated `VERIFIED` statements in this chronological ledger are historical provenance, not evidence for
the current exact hash. That final precondition is the authoritative current-candidate matrix.

Before Async v1 can be called complete:

- deterministic spawn/wakeup/timer ordering is specified and tested — **VERIFIED 2026-09-16** by
  FIFO reschedule, monotonic timer, registration-order, and simultaneous deadline/readiness tests;
- nested child ownership within the single v1 run scope, multiple awaiters, self-await, cancellation
  races, duplicate-source wake idempotence, stale task IDs, descriptor reuse, and exception paths
  have focused tests — **VERIFIED 2026-09-16** (nested `run()` remains rejected by design);
- every task-owned PHP value is released exactly once on success, failure, and cancellation —
  **VERIFIED 2026-09-16** by 55 focused heap-debug/GC scenarios;
- a blocking syscall is demonstrated to block the scheduler and documented as such — **VERIFIED
  2026-09-16** by the simultaneous expired-deadline/blocking-writer reducer and Async docs;
- the Fiber HTTP showcase or an equivalent socket reducer proves useful I/O interleaving —
  **VERIFIED 2026-09-16** by the socket-pair readiness/interleaving scenarios;
- monitoring distinguishes runnable, running, and blocked time — **VERIFIED 2026-09-16** by
  `test_cli_monitor_reports_async_scheduler_tasks`;
- macOS AArch64, Linux AArch64, and Linux x86_64 focused execution passes — **VERIFIED 2026-09-16**
  with all 34 Async functional scenarios on each executable host architecture;
- iOS compile behavior is explicit and tested — **VERIFIED 2026-09-15** (`--check` accepted,
  static-library Async execution refused on device and Simulator with host-contract guidance);
- strict-PHP hides the Elephc-only surface consistently in AOT and eval — **VERIFIED 2026-09-16**
  by the generated-prelude parity/surface suite.

Before Parallel v1 can be called complete:

- every worker has a distinct context, arena, stack limits, exception state, GC state, buffers, and
  resource tables — **VERIFIED 2026-09-21**: a synchronized two-worker C host writes distinct
  markers to the same index of every descriptor/native/wrapper/filter/chunk/connection table and
  to the non-resource arena, exception, GC, serializer, unserializer, and output scalar families.
  Each worker reads only its own marker before both contexts are released and reacquired on macOS
  AArch64, Linux x86_64, and Linux AArch64. Zlib, iconv, TLS, user-wrapper handles, user-filter
  instances, chunk sizes, and connection-host metadata are therefore directly isolated too.
  Process-global registration catalogs remain shared by design. Fatal shutdown uses the bounded
  per-context ownership registry and its closure is verified below;
- object/buffer and serialization side-table ownership is resolved before a second context allocates —
  **VERIFIED 2026-09-16** through per-context table pointers/cursors and the eight-argument
  fail-closed context-acquisition ABI;
- the checker rejects every non-transferable capture/argument/result path, including indirect
  callable forms — **VERIFIED LOCALLY 2026-09-20**: the focused 18-test Parallel diagnostic
  slice proves transitive global access, direct static-local/static-property access, nested scope
  rejection, captures, arguments, and returns; three additional public-prelude `Parallel::run()`
  diagnostics prove the same global/static rejection after full surface injection;
- cyclic arrays copy safely or are explicitly refused with a precise diagnostic — **VERIFIED
  2026-09-16** by the reference-aliased-cycle reducer;
- result and failure transfer occurs before context release and leaks no worker-owned pointer —
  **VERIFIED 2026-09-21**: an executor worker publishes both actual PHP-serialized success and
  failure payloads; parent `wait()` joins the worker and observes the context release before
  reading either payload, proving the retained bytes do not borrow the released worker context.
  Heap-debug now covers enum singleton shutdown, successive callback-return widening, and direct
  `TaskFailure`/`TaskGroupFailure` ownership. The following 3-block/4-block retention numbers are
  historical pre-fix diagnostics from 2026-09-21, not the final state: they were followed by the
  named-function activation and generic caller-cleanup fixes documented below. The end-to-end
  worker-failure reducer at that pre-fix point retained parent-side exception/drain owners, not
  worker-context resources (the preceding context-isolation/transfer tests passed); it retained
  3 blocks (360 bytes), while a root callback failure retains 4 blocks (200 bytes). The smallest
  generic `catch (Throwable) { throw $caught; }` reducer retains 3 blocks (160 bytes), proving
  that the remaining issue is the backend's generic rethrow ownership rather than Parallel;
  **FOCUSED RECHECK 2026-09-20 (pre-fix):** the smaller caught worker-failure probe retained 2
  blocks (80 bytes). **UNSET MATRIX RECHECK 2026-09-20:** keeping both catch locals, unsetting only
  `$failure`, unsetting only `$groupFailure`, and unsetting both all finish at exactly 2 live
  blocks / 80 bytes; `unset($failure)` adds one allocation and one free, while
  `unset($groupFailure)` is neutral. The leaked owners therefore predate the user catch body and
  are not retained in the released worker context. **EIR/ASM RECHECK 2026-09-20:** the reducer
  releases `$failure`, then stores boxed-Mixed `null`, and its main epilogue calls
  `__rt_decref_mixed` on that slot. The remaining owner must be established dynamically in the
  parent-side structured-run/failure path before changing the generic store/unset protocol.
  **LIFECYCLE MATRIX RECHECK 2026-09-20:** a successful `Parallel::run` and direct construction
  of a `TaskFailure` inside a `TaskGroupFailure` are heap-clean; a worker failure caught without
  reading either catch variable initially retained 2 blocks / 80 bytes. **ENUM COERCION FIX
  2026-09-20:** `src/ir_lower/expr/static_method_calls.rs` now releases an owning argument after
  successful `EnumBackingMixedToInt`/`EnumBackingStringToInt` coercion. The generic heap-debug
  regression `test_int_backed_enum_from_mixed_array_read_is_heap_clean` is green for both inline
  owning and parameter-borrowed mixed arguments; all five `parallel_worker_` tests
  remain green. The Parallel reducer was then down to 1 block / 40 bytes. **RESULT-BOX MATRIX
  RECHECK 2026-09-20:** an `int`, `null`, or array returned by the root body each leave exactly
  the same 1 block / 40 bytes after a worker failure, excluding `$resultBox`'s representation as
  the owner. Direct and relayed `__CancellationState::request(null)` reducers are heap-clean,
  excluding the default `?Throwable` cancellation parameter; the prepared native-buffer
  `ptr → string → unserialize` path and private typed `TaskFailure` property transfer are clean
  too. Three failed scopes retain three blocks, while three failed workers in one scope retain one,
  proving an owner per failed scope rather than per Future. The remaining dynamic owner is therefore
  specific to the failed scope’s drain/join lifecycle before `run()` constructs or throws the group
  exception. **NAMED-FUNCTION ACTIVATION FIX 2026-09-20:** ordinary user functions now publish
  the same owning-local cleanup activation as closures. The generic callee-throw heap regression
  is green, and this closes the final failed-scope owner: both worker PHP-wire heap tests now pass
  as ordinary, non-ignored tests. **DESCRIPTOR INVOKER ESCAPE FIX 2026-09-20:** a dynamic closure
  invoker now retains its caller-owned boxed argument container across its exception boundary,
  releases it on longjmp, then rethrows; its eval-null escape releases the same container. The
  dynamic-closure rethrow and root-body Parallel failure regressions are now ordinary green tests.
  **EXECUTABLE MATRIX RECHECK 2026-09-21:** the focused `parallel_` suite passes 34/34 on macOS
  AArch64, Linux x86_64, and Linux AArch64 with the final cleanup/invoker code.
  **GENERIC CALLER-CLEANUP FIX 2026-09-22:** the caller cleanup handler now covers user calls that
  carry owning closure/string/array/Mixed temporaries and rethrow. The two former ignored generic
  closure-rethrow heap-debug reducers are ordinary green regressions; no retained temporary is
  waived from this scheduler candidate;
- cancellation worked while queued, running cooperatively, completing, and transferring a result on
  the historical candidate — **HISTORICAL 2026-09-20**: the former queued/running lifecycle coverage
  was joined by a
  deterministic PHP-wire race: the worker has decoded its serialized result into owned EPV1
  bytes, the parent records cooperative cancellation, and only then may the worker publish. The
  terminal outcome remains the valid result, proving that cancellation cannot erase an in-flight
  transfer after the worker has reached the completion boundary;
- concurrent context acquisition/release is race-tested on each executable host architecture —
  **VERIFIED LOCALLY 2026-09-20**: the staticlib C host fills the pool, releases exactly one slot,
  gates eight pthread contenders simultaneously, and proves exactly one acquires the released slot.
  The exact C-host race passes on macOS AArch64, Linux x86_64, and Linux AArch64;
- bridge panics and PHP fatals become typed parent-side failures without crossing the C ABI —
  **VERIFIED 2026-09-21**: a controlled Rust panic after context acquisition is converted to
  `WorkerPanic`, wakes the waiter, joins the worker, releases the context exactly once, and permits
  job release. The PHP-fatal trampoline now clears stale catch authority, runs every activation
  cleanup, releases a pending Throwable, and collects cycles on both ABIs before returning to
  Rust. The allocation-owning `exit(7)` worker and a destructor that allocates a Buffer then issues
  a second `exit()` both reach typed `PhpFatal`; the controlled `WorkerPanic` executor test and
  the focused `parallel_` execution matrix pass on macOS AArch64, Linux x86_64, and Linux AArch64,
  and the generated runtime assembles for every supported target. The generic inline-closure
  temporary rethrow path is closed by the ordinary caller-cleanup regressions above. The implemented descriptor/resource owner
  families are drained through a direct fatal-host matrix on every executable architecture;
- nested Parallel scopes fail deterministically rather than exhausting all worker slots and
  deadlocking — **VERIFIED 2026-09-21** by direct/transitive/assigned-closure diagnostics, the
  bridge worker-active lifetime test, generated-prelude parity, and the focused `parallel_` matrix
  on macOS AArch64, Linux x86_64, and Linux AArch64;
- `Future` construction and task-group bookkeeping authority are unforgeable, while an escaped
  settled Future remains repeatable and heap-clean after native thread/context drain — **VERIFIED
  2026-09-21** by private-constructor/private-hook diagnostics, authority execution, repeated
  post-scope join, heap-debug regressions, and the focused `parallel_` matrix on macOS AArch64,
  Linux x86_64, and Linux AArch64;
- root and child failures are both retained when they happen in one scope — **VERIFIED 2026-09-21**:
  a lone root Throwable is rethrown unchanged, while a concurrent root plus child failure produces
  `TaskGroupFailure::rootFailure()` and submission-ordered `failures()` on the focused `parallel_`
  matrix for macOS AArch64, Linux x86_64, and Linux AArch64;
- worker codec/allocation statuses preserve `ArenaExhausted`, `TransferEncode`, `TransferDecode`,
  and infrastructure distinctions end to end — **VERIFIED 2026-09-21** by generated callback
  mapping, executor terminal-envelope tests, the focused `parallel_` matrix on macOS AArch64,
  Linux x86_64, and Linux AArch64;
- a proven `Future::join()` in an Async root or spawned task warns that it blocks the cooperative
  scheduler thread, without warning for synchronous joins or merely nested, unexecuted closures —
  **VERIFIED LOCALLY 2026-09-17** by six focused Async diagnostics and the 23-test Parallel
  diagnostic slice;
- focused target tests prove both architectures rather than emitted-text similarity alone —
  **VERIFIED 2026-09-21**: `parallel_` passes 34/34 on macOS AArch64, Linux x86_64, and Linux
  AArch64 after the reopened fixes; the context-isolation C host and controlled `WorkerPanic`
  executor test each replay on that same executable matrix. iOS analysis/refusal remains explicit
  on device and Simulator.

Cross-cutting acceptance corrections from the first exact-hash review:

- preserve the verified checker warning for `Future::join()` in a provably Async task;
- preserve the strengthened monitoring test: it parses rendered task rows and proves
  scenario-appropriate nonzero runnable/running/blocked time (**VERIFIED LOCALLY 2026-09-20**);
- replace any broad host-matrix claim with commands/artifacts tied to the exact reviewed tree.

### Historical reconciled target ledger — 2026-09-21

The following commands are reproducible evidence for the 2026-09-21 reconciled candidate only.
Every `parallel_` **34/34** count in this ledger and its preceding dated entries is historical,
valid for that candidate's test slice, and superseded as current acceptance evidence by Candidate 35's
complete `parallel_` **58/58** runs on macOS AArch64, Linux x86_64, and Linux AArch64. Candidate 40's
matrix, restated in the current review precondition below, is the sole authoritative current
end-to-end scheduler matrix; these historical counts are not added to or compared as if they were the
same test slice.
They predate the 2026-09-22 ownership, scope-entry, and blocking-Parallel-warning amendments, and
are retained for historical traceability rather than claimed as current-candidate acceptance.
The current-candidate matrix is recorded separately in the final exact-hash precondition before the
consensus procedure, which supersedes every earlier candidate ledger.
They must be run from the dedicated Scheduler worktree with a clean Docker virtual disk; each
Linux command reuses the named target volume only to avoid a fresh-volume disk exhaustion, not to
change the test scope.

```sh
CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUST_MIN_STACK=67108864 \
  cargo test --test codegen_tests parallel_ -- --nocapture

ELEPHC_DOCKER_TARGET_VOLUME=elephc-target-linux-x86_64-8067abf45272ac3f-23391 \
ELEPHC_KEEP_DOCKER_TARGET_VOLUME=1 CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 \
RUST_MIN_STACK=67108864 ./scripts/test-linux-x86_64.sh parallel_

ELEPHC_DOCKER_TARGET_VOLUME=elephc-target-linux-arm64-8067abf45272ac3f-8593 \
ELEPHC_KEEP_DOCKER_TARGET_VOLUME=1 CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 \
RUST_MIN_STACK=67108864 ./scripts/test-linux-arm64.sh parallel_

# Reconciled worker-context isolation host
cargo test --test cdylib_tests \
  test_rt_ctx_pool_hands_out_distinct_slots_and_reuses_released_ones

# Controlled post-acquisition bridge panic
cargo test -p elephc-parallel \
  controlled_bridge_panic_wakes_waiter_joins_worker_and_releases_context
```

The first command passed 34/34 on macOS AArch64. Both Linux `parallel_` commands passed 34/34.
The isolation host and controlled-panic test each passed on every executable architecture. The
generic inline-closure rethrow leak that earlier acceptance notes excluded is now closed by
ordinary heap-debug regressions; the public scheduler matrices therefore rely on no waived
temporary-owner path.

**Scope-capability recheck 2026-09-21:** `cargo test --lib parallel_prelude -- --nocapture`
passes 5/5, including PHP-oracle/AST parity and the explicit closed-state guard contract;
`cargo test --test error_tests parallel_task_group_cannot_escape_its_root_scope_even_through_mixed
-- --nocapture` passes 1/1; and the macOS `parallel_` executable matrix remains 34/34 after
compiler-lowered `spawn()` gained the same guard.

**Post-scope failed-Future recheck 2026-09-21:** the new executable reducer stores a worker Future
outside the root body, catches the scope `TaskGroupFailure`, then calls `join()` on that escaped
handle. It rethrows the preserved parent-side `TaskFailure` (`LogicException|worker-boom`) rather
than reading an unset typed property. The reducer and the complete `parallel_` matrix pass 34/34 on
macOS AArch64, Linux x86_64, and Linux AArch64; all three filtered diagnostic matrices also accept
the scope-capability rejection.

### Fatal resource-drain ledger — VERIFIED FOR IMPLEMENTED V1 OWNERS

The fatal trampoline must not confuse resetting a context with releasing what the context owns.
The following evidence distinguishes real close operations from synthetic table wiring:

- **DONE:** flush output buffers before the fatal longjmp;
- **DONE:** unwind activation cleanup owners, release the pending Throwable, and collect cycles;
- **DONE:** route popen, directory, glob, bzip2, zlib, iconv, TLS, user-wrapper,
  user-filter, chunk-size, and connection-host tables through the active `_rt_ctx`;
- **DONE (creation bookkeeping):** a bounded per-context ownership registry is marked at the
  shared stream-resource boxing boundary, covering ordinary files, pipes, directories, temporary
  and data streams, FTP/HTTP, sockets, accepted descriptors, and related single-handle surfaces;
  `socketpair()` claims both descriptors before allocating its result. Explicit close and scope
  cleanup clear ownership before descriptor reuse. Scanning all process descriptors remains
  forbidden because another worker or the parent may own them;
- **VERIFIED LOCALLY 2026-09-20:** fatal cleanup scans all 4,096 Buffer descriptors, detaches each
  active slot before returning its payload to the worker arena, and skips inactive/retired
  descriptors on both ABIs. A destructor that allocates and mutates a Buffer before issuing a
  second worker fatal stays contained and reaches the typed parent-side `PhpFatal` path on macOS
  AArch64; runtime assembly validates both ABIs;
- **VERIFIED:** after activation cleanup and cycle collection, the fatal trampoline scans only the
  current context's ownership registry, detaches each slot before reuse, closes zlib/bzip2/iconv/TLS
  state, then closes/reaps popen, directory/glob, and ordinary stream owners exactly once on both
  ABIs. User-filter read/write slots now detach before callbacks, deduplicate a shared instance,
  release each table owner exactly once, and use a fatal-only no-`onClose` abandon path; destructor
  re-entry with a live Buffer is executable and contained;
- **VERIFIED LOCALLY (fail-closed):** `__rt_ctx_release` republishes the worker context, reruns the
  idempotent stream/Buffer/GC drains, scans scalar owners, every native/wrapper/filter table,
  descriptor bytes, connection metadata, and all Buffer active markers, then atomically frees the
  slot only when every check is empty. The C host injects a dirty wrapper owner into an otherwise
  full pool and proves release returns failure and the slot remains unavailable. The Rust release
  guard then reports `PARALLEL_WORKER_CLEANUP_FAILED` and deliberately retains the `WorkerStorage`
  backing rather than leaving the quarantined context with dangling pointers. Fatal wrapper-object
  abandonment now scans and detaches every wrapper slot before decref; direct wrapper-registry
  access from a Parallel task is rejected as process-global storage, so the supported v1 surface
  cannot progressively reduce pool capacity through wrapper state.
- **VERIFIED 2026-09-21 (ordinary descriptor, fatal path):** the static-library C host opens
  `/dev/null` from an installed worker callback, marks that real descriptor through the production
  result-register ABI, then calls `__rt_exit_or_parallel_fatal(7)`. The production trampoline
  returns the typed `PhpFatal` status and closes the descriptor before the host calls
  `__rt_ctx_release` (`fcntl(..., F_GETFD)` already reports `EBADF`); release then returns the
  exact pool slot for reuse on macOS AArch64, Linux x86_64, and Linux AArch64. This proves generic
  descriptor closure and reuse on the true fatal path, but does not replace separate fatal execution
  evidence for every optional native filter/TLS table.
- **VERIFIED 2026-09-21 (optional native-handle dispatch):** the same fatal C host places a
  non-null entry for its owned descriptor into each zlib, bzip2, iconv, and TLS table, installs C
  hooks through the production `_zlib_close_fn`, `_bz2_close_fn`, `_iconv_close_fn`, and
  `_elephc_tls_close_fn` cells, and then escapes through `__rt_exit_or_parallel_fatal(7)`. Every
  hook receives its reviewed argument; the zlib/bzip2/iconv hooks clear their own slots and the
  runtime clears the TLS slot after its handle-based hook. All four tables are empty before release
  and the context is reusable on macOS AArch64, Linux x86_64, and Linux AArch64. This proves the
  optional dispatch/ownership protocol, while bridge functionality and other resource-table
  families remain independently covered or open.
- **VERIFIED 2026-09-21 (real pipe and directory closers):** separate fatal-worker callbacks open
  a real libc `popen()` pipe and a real `DIR*`, record their `FILE*`/`DIR*` in the context table,
  and escape through `__rt_exit_or_parallel_fatal(7)`. `__rt_pclose` reaps the child and clears the
  pipe entry; `__rt_closedir` clears the directory entry and closes its descriptor. Both descriptors
  report `EBADF` before context release, and each slot is reusable on macOS AArch64, Linux x86_64,
  and Linux AArch64. The synthetic `glob` state and user-owned wrapper/filter object tables remain
  separate gates.
- **VERIFIED 2026-09-21 (real glob closer):** a fatal-worker callback allocates the same
  24-byte-prefix-plus-`glob_t` state expected by `__rt_closedir`, fills it with a real libc
  `glob("*")` result, associates it with an owned `/dev/null` descriptor, then escapes through
  `__rt_exit_or_parallel_fatal(7)`. The glob branch clears the table, calls `globfree`, frees the
  state, and closes the descriptor before release; the context is reusable on macOS AArch64, Linux
  x86_64, and Linux AArch64. User-owned wrapper/filter object tables remain separate gates.
- **VERIFIED 2026-09-21 (wrapper owner table):** a fatal-worker callback creates an owned Elephc
  string through the production `__rt_cstr_to_str` ABI and stores it in a wrapper-handle slot. The
  worker trampoline’s `__rt_user_wrapper_abandon_all` loop detaches the slot and dispatches its
  release through `__rt_decref_any`; `__rt_ctx_release` accepts and reuses the context on macOS
  AArch64, Linux x86_64, and Linux AArch64. This structurally proves table detachment and valid
  heap-owner release, while PHP wrapper-object method/destructor semantics remain covered by their
  dedicated tests.
- **VERIFIED 2026-09-21 (user-filter owner and metadata tables):** a fatal-worker callback places
  one valid heap owner in both read/write instance slots for a marked descriptor. The fatal-only
  filter cleaner clears both slots, releases the shared owner once, closes the descriptor, and
  admits the context again on macOS AArch64, Linux x86_64, and Linux AArch64. The same host seeds
  EOF, read/write filter IDs, chunk size, and connection-host pointer/length metadata and proves
  every entry is zero before release. Together with the ordinary FD, native, pipe, directory, glob,
  Buffer, and wrapper cases above, this directly covers every owner family scanned by
  `__rt_ctx_verify_drained` for the implemented v1 runtime.

## Consensus questions

Implementation may proceed in independently verified slices under the maintainer's explicit
authorization. An unresolved question remains provisional and must not be silently promoted to a
locked public contract merely because a first implementation needs temporary behavior.

1. [x] **LOCKED 2026-09-13:** `Async\run()` drives the root scope and returns its final value,
   replacing the older `run(): Awaitable` sketch.
2. [x] **LOCKED 2026-09-13:** `TaskGroup::spawn()` is the only public task-creation operation in v1;
   detached tasks are structurally forbidden.
3. [x] **LOCKED 2026-09-21:** `Cancellation` is observation-only;
   `Async\TaskGroup::cancel()` and `Parallel\TaskGroup::cancel()` each own group-wide cancellation
   authority only while their owning scope is active. The first request wins. A Parallel cancellation reason stays parent-local, while a
   worker-local `CancelledException::reason()` is `null`.
4. [x] **LOCKED 2026-09-13:** the default TaskGroup fails fast, cancels siblings, and drains
   their cleanup before propagating an unobserved failure.
5. [x] **LOCKED 2026-09-13:** catching an exception delivered by `Awaitable::await()` marks that
   task failure handled for scope-exit purposes without rolling back sibling cancellation.
6. [x] **LOCKED 2026-09-13:** multiple awaiters of one Async result are supported in v1.
7. [x] **LOCKED 2026-09-13:** explicit fairness remains
   `TaskGroup::reschedule()`; v1 does not add a namespace-level helper or process-global scheduler
   lookup.
8. [x] **LOCKED 2026-09-13, clarified 2026-09-25:** the monotonic timer operation remains
   `TaskGroup::sleep(float $seconds)`; positive and negative IEEE-754 zero compare as zero and are
   reschedule points. Strictly negative finite values, NaN, infinity, and durations outside the
   monotonic deadline range raise `ValueError` before suspension or registration.
9. [x] **LOCKED 2026-09-13:** v1 uses a dependency-free libc `poll()` backend plus
   `TaskGroup::awaitReadable/awaitWritable(mixed $stream, ?float $timeout = null): bool`, retaining
   a backend-neutral boundary for a later epoll/kqueue upgrade.
10. [x] **LOCKED 2026-09-16, EXTENDED 2026-09-22:** `Future::join()` and
    `Parallel\run()` remain legal, explicitly blocking operations. When the checker proves
    `Future::join()` inside an Async task it warns that the scheduler thread blocks. A proven
    `Parallel\run()` call inside Async warns that it is synchronous and returns only after scope
    drain; explicitly reached Async scheduler yield points in its body may still run tasks, but
    there is no implicit offload or preemption. It does not reject generic code whose eventual
    calling context may be synchronous, an ordinary user-created Fiber, or Async.
    `Parallel\run()` runs from an ordinary Fiber and keeps its parent body in that calling Fiber;
    focused execution evidence pins this. No implicit Future/Awaitable
    conversion is added. A later explicit `Async\awaitFuture()` adapter remains the non-blocking path.
    When that parent call originates inside an Async task, the Parallel parent body remains in the
    caller's `_rt_ctx`; explicit Async scheduler yields it reaches may run unrelated Async tasks in
    that same context. This does not extend the worker-only global/static restriction in question 16:
    that restriction applies solely to callback call graphs executed inside isolated Parallel
    workers, not to the Parallel parent closure or Async siblings.
11. [x] **LOCKED 2026-09-16:** the native v1 failure envelope is versioned
    and contains `kind`, nullable remote class, message, integer code, nullable file, line, and an
    ordered list of available `{function, file, line}` compiled frames. It never contains a remote
    Throwable/object pointer. Public parent-side types are:
    - `TaskFailureKind` for `PhpThrowable`, `PhpFatal`, `WorkerPanic`, `ContextUnavailable`,
      `ArenaExhausted`, `TransferEncode`, `TransferDecode`, and `Infrastructure`;
    - `TaskFailure extends RuntimeException`, thrown by the corresponding `Future::join()`.
      `ContextUnavailable` specifically means that an admitted worker's configured context-acquire
      callback returned null, never ordinary queue saturation;
    - `TaskGroupFailure extends RuntimeException`, whose `failures(): array` is always ordered by
      submission and is thrown at scope exit for remaining unobserved child failures (including a
      one-element aggregate, so callers do not branch on count).
12. [x] **LOCKED 2026-09-16:** v1 creates one owned platform thread per
    admitted task and joins every such thread before its `Parallel\run()` scope returns. Admission is
    bounded by the internal context pool and queued FIFO without a v1 queue-length cap; contexts/
    side-table allocations may be reused, but OS threads are not persistent. This keeps
    `Parallel\Runtime` and a reusable worker
    pool out of v1 as already arbitrated, while leaving the executor-neutral job/worker ABI reusable
    by that later runtime.
13. [x] **LOCKED 2026-09-17:** nested `Parallel\run()` inside a Parallel worker is rejected in v1,
    statically when provable and at runtime otherwise. A reentrant executor is deferred; bounded
    admission must never turn nesting into a pool-exhaustion deadlock.
14. [x] **LOCKED 2026-09-17, REVISED AFTER REVIEW:** `Future` is
    compiler/runtime-constructed; raw native job IDs and task-group mutation hooks are private and
    are not PHP user authority. A Future may escape its owning `Parallel\run()` only after scope
    drain has joined the worker and settled the actual handle. The escaped handle owns copied
    terminal native bytes/envelopes, releases that storage in its destructor, and supports
    repeatable post-scope joins without retaining a worker context or arena pointer. A failed
    escaped handle retains its parent-side `TaskFailure` until its own destruction, so joining it
    after a caught scope aggregate rethrows that same typed failure.
15. [x] **LOCKED 2026-09-17:** a combined root/child failure leaves the scope as
    `TaskGroupFailure`, preserving the root Throwable through `rootFailure()` and every unobserved
    child `TaskFailure` through submission-ordered `failures()`.
16. [x] **LOCKED 2026-09-17:** Parallel worker call graphs may not access process-global PHP
    storage in v1: globals, static locals, and static properties are rejected transitively. Moving
    those storage classes into `_rt_ctx` is a separate future design, not an implicit v1 shortcut.
    The restriction is scoped only to call graphs executed by isolated worker callbacks; it does not
    apply to the Parallel parent closure or same-context Async tasks that run at explicit yield points
    under question 10. The checker enforces the restriction at worker-task admission, not by treating
    every call reachable from `Parallel\run()` as worker code. This storage rule is distinct from
    question 19's transfer rule: ordinary by-reference captures remain rejected, with only the direct
    Async `Cancellation` token capture/argument as its explicit read-only exception. The checker
    computes this restriction per worker-task admission/call path, not as a global property of the
    callable symbol: if one helper is reachable from both a Parallel worker spawn and a nested Async
    task in the parent `_rt_ctx`, the Parallel admission is rejected for global/static access while
    the same helper remains permitted on the same-context Async path. The worker finding does not
    poison unrelated parent/Async call sites.
17. [x] **LOCKED 2026-09-17:** every worker termination path, including PHP fatal and Rust panic,
    drains worker-owned PHP values, descriptors, resource tables, buffers, and GC state before the
    context is reset and returned to the pool. Resetting only scalar context fields is insufficient.
18. [x] **LOCKED 2026-09-21, REVISED AFTER REVIEW:** only settled `Future` handles may escape
    `Parallel\run()`. `TaskGroup` has a private constructor and is a scope capability created only
    by `run()`: statically proven returns are rejected, and its closed-state guard rejects every
    post-scope public or compiler-lowered operation before a payload or native job can be created.
19. [x] **LOCKED 2026-09-23:** Async `Cancellation` is transferable to a Parallel worker only as
    one direct `spawn()` argument or one direct closure capture, including the named by-reference
    capture. It is removed from the PHP/EPV payload and reconstructed against the worker job.
    Cancellation values inside arrays or unions and all Cancellation task results are rejected by
    the checker before codegen.
20. [x] **LOCKED 2026-09-23:** the Async root Fiber is not implicitly interrupted at its own
    scheduler yield points after a group cancellation request. Awaiting a cancelled child or
    explicitly calling `Cancellation::throwIfRequested()` throws the request-paired exception. If
    that exception escapes root code, it is an ordinary root failure and `Async\run()` rethrows the
    same exception object after draining children when it is first in failure occurrence order;
    an earlier unobserved child failure remains primary. Catching the root exception leaves the root
    free to return.
21. [x] **LOCKED 2026-09-23, REVISED AFTER REVIEW:** `Parallel\run()` is supported from an ordinary
    user-created Fiber, but executes synchronously through scope drain: user-level `Fiber::suspend()`
    is rejected before argument evaluation while a Parallel parent scope is active on that OS thread.
    The same fail-closed guard applies on isolated Parallel worker threads. A thread-local parent
    scope guard is entered and left with the owning group. The Async scheduler's own
    `__Scheduler::awaitTask()` and `rescheduleCurrent()` yields are exempt and remain
    scheduler-owned; raw user suspension does not transfer a live Parallel scope to an external
    resumer. Async-task calls to `Parallel\run()` remain legal with the blocking warning.

## First exact-hash review outcome — 2026-09-17

Three independent read-only reviews locked
`f3b18f91c6c3381ebb871d23dbb5e68d6df33d85e427eaef7f98db808c8bc7a5` before inspection. The
review round is valid, but the candidate is rejected. Its blocking findings are retained above as
normative decisions or reopened acceptance gates:

- bounded-worker deadlock under nested Parallel scopes;
- forgeable `Future` IDs and publicly callable task-group authority;
- unusable escaped Futures after scope drain;
- discarded child failures when the root also throws;
- process-global `global`/static storage reachable from isolated workers;
- fatal worker paths returning contexts without a complete cleanup/drain;
- codec/allocation statuses collapsing into `Infrastructure`;
- missing `Future::join()`-inside-Async warning (**closed locally 2026-09-17**);
- overstated acquisition-race, monitoring, pointer-lifetime, cancellation-race, and cross-host
  acceptance evidence; the controlled-panic and PHP-fatal resource-drain portions are closed by
  their focused host matrices, while the remaining claims stay gated independently.

The old companion hash and the second-review lock are intentionally invalidated. The manual
Parallel-cancellation finding is resolved by the public group-wide cancellation API, its
parent-local reason contract, and its three-target focused matrix. Freeze another hash for an
independent review round.

## Second exact-hash review outcome — 2026-09-21

The reviewer locked
`b713377ddabfbc467370dfde4b6aafdb4d900630aff39a8db6704879c0a95066` before inspection and
identified three contract gaps: the public API listing had omitted `TaskGroup::cancel()`, the
terminal behaviour of a cancelled `Future::join()` was not explicit enough, and an escaped
`TaskGroup` had no defined post-scope behaviour. The first two are reconciled in the API and
cancellation sections above. The third is resolved by locked decision 18, the direct checker
diagnostic, and the prelude/lowering closed-state guard.

This amendment invalidates that hash. A fresh hash and a complete three-review round are required;
no prior acceptances count toward the next consensus.

## Third exact-hash review outcome — 2026-09-21

The next frozen candidate was
`f22eddc9f1f38d8523bbc4c25a2656f157bf3666f59ddb47163b4b64c3964ddf`. DeepSeek and Kimi both
locked it but rejected on readings contradicted by the supplied source: PHP arguments are evaluated
before a method body guard, phase 4 is the fail-fast failure phase, and `run()` drains before it
closes a group. The local PHP oracle prints `argument|closed` for that ordering. Their requests
therefore did not require a semantic change.

GLM's transcript was incomplete and cannot count as a review, but its analysis exposed a real
counterexample: `Future::__finishScope()` released the native job and also unset a failed escaped
handle's parent-side `TaskFailure`. The executable reducer now preserves that object through the
post-scope join, then releases it in `Future::__destruct`; its heap-debug and complete 34/34 matrix
pass on all three executable targets. This source and contract correction invalidates
`f22edd…c3964ddf`; freeze again and repeat a complete three-review round.

## Fourth exact-hash review outcome — 2026-09-21

The candidate
`2be67ded927a45f52205b58fa1bae534304692bba20d8e088677f470de866070` received a locked
unconditional acceptance from Kimi. DeepSeek locked the same candidate but rejected it because
`Async\TaskGroup::cancel()` and `cancellation()` bypassed the existing detached-scope guard.
That is a valid structured-authority gap: a retained Async group could still mutate or expose
ended scope cancellation state. The guard now covers both methods, while a token obtained during
the scope remains explicitly observation-only. The focused Async prelude and executable regressions
pass.

GLM did not return a final verdict, so it cannot count. This Async correction invalidates the
candidate hash; the next complete three-model round must use a newly frozen document.

## Fifth exact-hash review precondition — 2026-09-22

The provisional follow-up candidate
`fa0eccd377cd00aaf57fc38531994d3e723fbca8ce5e1f341be27c2536fb7317` is invalidated before a
new review round. Heap-debug acceptance exposed two independent ownership defects that must be
part of the next frozen candidate:

- A Fiber running a dynamic callable kept its descriptor-invoker argument box in both the invoker
  exception boundary and the Fiber escape recovery slot. The invoker already consumes the box when
  it rethrows, so the Fiber trampoline now clears that stale recovery slot instead of decrefing it
  a second time. The repair is target-symmetric (AArch64 and x86_64) and is covered by a generic
  dynamic-Fiber failure regression.
- `__Scheduler::runRoot()` only released its callable and group parameters on successful return.
  Its single error boundary now tears down task records and explicitly releases those parameters
  before rethrowing, including failures propagated by `throwFirstUnobservedFailure()`.

Focused host evidence after those repairs is green: Async scheduler/GC coverage is **108/108**;
the Async prelude AST/source agreement is **5/5**; Parallel execution is **16/16**, transfer is
**4/4**, and Parallel heap-debug ownership is **11/11**. The broad lexical `parallel_` filter also
exercised these cases, but its unrelated Curl HTTP fixture cannot bind a local port in this sandbox;
that environmental failure is not an acceptance result for the scheduler. The dynamic-Fiber
exception reducer is also green on all three executable hosts: macOS AArch64, Linux x86_64, and
Linux AArch64. No model-review verdict is currently valid. Freeze the amended document, then
require all three reviewers to return the same exact lock, `ACCEPT`, no findings, and no questions.

## Sixth exact-hash review precondition — 2026-09-22

Kimi K3 rejected the first Fifth-round candidate without locking it. Its findings were actionable
document inconsistencies, not a new scheduler behavior failure: the relationship between the
already-open v1 implementation gate and intentionally deferred work was unclear; historical Linux
bridge language conflicted with the current executable matrix; iOS fail-closed behavior was not
distinguished from executable readiness; and two generic closure-rethrow ownership reducers were
still marked ignored. This amendment explicitly scopes the candidate, removes the stale bridge
claim, states the iOS policy, and closes the generic user-call cleanup gap with ordinary
heap-debug regressions. The representative generic rethrow reducer passes on macOS AArch64, Linux
x86_64, and Linux AArch64. It invalidates every prior hash and model response. The next round
starts only with the companion hash generated from this amended document.

## Seventh exact-hash review precondition — 2026-09-22

GLM 5.3 rejected the next candidate after a complete review. Its semantic findings are now closed
by explicit contract rules and executable regressions:

- Async scope failure order includes the root task and every child. The first unobserved failure in
  actual occurrence order is the single propagated `Throwable`; a new root-after-child regression
  proves that a child-first `LogicException` wins over the later root failure.
- A direct self-await throws the named `Error` immediately, while a longer wait cycle with no
  runnable or wake source becomes the documented Async deadlock error. A deliberate group
  cancellation is not an unobserved failure: a normally returning root drains cancelled children
  and returns its value, now covered by a regression.
- `TaskFailure` is a PHP-visible public diagnostic type. Its `kind()`, `remoteClass()`,
  `remoteFile()`, `remoteLine()`, and `remoteFrames()` accessors are listed in the public API;
  `TaskGroupFailure::failures()` and `rootFailure()` expose the aggregate.
- A Parallel worker may not directly or transitively enter `Async\run()` in v1. The shared
  fixed-point callable analysis records that scope entry and the transfer checker rejects it before
  codegen, preventing the previously reproducible worker SIGSEGV.

The current focused executable matrix is green on every executable host: macOS AArch64, Linux
x86_64, and Linux AArch64 each pass Async **108/108**, Parallel execution **16/16**, and Parallel
heap-debug ownership **11/11**. The direct and transitive worker-Async diagnostics each pass on
the same three hosts, so the new fail-closed boundary is checked before target code generation as
well as documented. Every prior hash and review response is invalidated. The next review round
must lock the companion hash generated from this amended document.

## Eighth exact-hash review precondition — 2026-09-22

GLM 5.3 locked and rejected the Seventh-round candidate
`a7f6e0a79b74ce6535fd259f70f7c675180024f463c2e9497ca36741ac1099a2`. Its review correctly
separated an incomplete current-evidence claim from several implemented-but-undocumented terminal
rules. The reconciliation is deliberately complete:

- The 2026-09-21 `parallel_`/C-host ledger is relabelled historical. It is not current-candidate
  evidence. The reviewed executable matrix is instead the explicitly enumerated three-host Async
  **108/108**, Parallel execution **16/16**, and Parallel heap **11/11** reducers above, plus the
  direct/transitive worker-Async rejection reducers. `VERIFIED LOCALLY` fatal/transfer/quarantine
  probes elsewhere in this historical ledger are macOS AArch64 diagnostic evidence only unless a
  line explicitly names Linux x86_64 and Linux AArch64; they neither stand in for nor silently join
  the current executable matrix.
- `Awaitable::isComplete()` and `Future::isComplete()` return `true` for normal, Failed, and
  Cancelled terminal states without observing or consuming a failure. The Parallel execution
  reducers now exercise both failed and cancelled `Future` handles.
- Negative descriptor timeouts raise `ValueError`; the existing descriptor validation regression
  already proves it. Cross-category ready-queue order is specified phase by phase, including the
  ordering after a blocking `poll()` returns.
- A `TaskGroupFailure::failures()` entry is precisely an unobserved Failed child in submission
  order, regardless of whether fail-fast cancellation was already requested; a Cancelled outcome
  is never a `TaskFailure`. `remoteLine()` returns `0`, with null file/class, for a location-less
  failure kind.
- A Parallel cancellation reason is observable only in the active parent scope through an acquired
  `Cancellation::throwIfRequested()` and its `CancelledException::reason()`. It never crosses into
  a worker, and scope close clears it. The executable cancellation reducer now proves both the
  parent reason and worker-null behavior.
- `Parallel\run()` inside a statically proven Async task receives the same blocking-scheduler
  warning as `Future::join()`, with a focused checker regression.

The Eighth amendment changes the checker and public contract, not Async/Parallel runtime lowering.
Its exact focused evidence is complete: macOS AArch64 passes the Async scheduler diagnostic slice
**7/7** and Parallel execution **16/16**; Linux AArch64 and Linux x86_64 each pass the new
`Parallel\run()`-inside-Async warning reducer **1/1**, the cancellation/reason and cancelled-Future
reducers **2/2**, and the failed escaped-Future reducer **1/1**. Those target reducers exercise the
new contract assertions directly; the wider current runtime matrix remains the explicitly scoped
Async **108/108**, Parallel execution **16/16**, and heap **11/11** evidence above.

This code/specification amendment invalidates every previous hash and transcript. Freeze a fresh
companion hash, then start a new full three-model review round.

## Ninth exact-hash review precondition — 2026-09-22

Kimi K3 locked and rejected the Eighth-round candidate
`b55155c69a2018375a00017cd37ee8fa729d8301b533598fa2b724566a8721c0`. The rejection exposed a
real asymmetry: direct/transitive worker calls into `Async\run()` were checker-rejected, but a
runtime-resolved `call_user_func` path could reach the injected surface. The new Parallel-only AST
patch adds the symmetric runtime guard to `Async\run()` exactly when the Parallel bridge has also
declared `elephc_parallel_worker_active`; Async-only programs do not gain a bridge dependency or an
unresolved native symbol. The guard throws
`Error("Elephc\\Async\\run() cannot be called from a Parallel worker in v1")` before state creation.

The fresh regression keeps an otherwise-reachable `Async\run()` declaration, then dynamically calls
it by name from a Parallel worker through `call_user_func`. The worker returns the typed parent-side
`TaskFailure` with that guard message. The complete Parallel prelude slice is **6/6** and Parallel
execution is **17/17** on macOS AArch64; the executable dynamic-dispatch reproducer also passes on
macOS AArch64, Linux AArch64, and Linux x86_64. The dangling “companion hash below” wording is
replaced by a direct link to the companion record. Every earlier hash and transcript is invalidated;
the next review starts only after a new companion hash is recorded.

## Tenth exact-hash review precondition — 2026-09-22

GLM 5.3 locked and rejected the Ninth-round candidate
`ad9790a3f424f7367bde3ec59a86a3daf0a2756d6439afce0160b3625129d2bd`. Its five cancellation
findings are reconciled in code and contract:

- The terminal publication boundary is exact. Queued native jobs become Cancelled at the request.
  A running job remains Running until it publishes a result/failure or exits; an already published
  result or genuine non-cancellation failure wins. The synthesized worker wrapper recognizes only
  an uncaught local `CancelledException` paired with the active native request, releases its local
  state without a failure envelope, and returns so the executor terminalizes Cancelled. The regression
  proves that this path does not produce `TaskFailure` or `TaskGroupFailure`.
- After cancellation is requested, both `TaskGroup::spawn()` surfaces throw a precise `Error` before
  creating a scheduler child, transfer payload, or native job. The Parallel compiler-lowered path
  calls the same private admission guard as the PHP surface.
- Async fail-fast uses the first request's reason: an escaping task Throwable becomes the
  shared-context cancellation reason only when that failure initiates the request; a prior explicit
  `cancel(null)` or `cancel($reason)` is not overwritten. Parallel workers always see a local
  cancellation with null reason: a root Throwable is retained parent-locally during drain, while
  child-triggered fail-fast sends only the native request bit.
- Escaped Async tokens retain their requested state and original reason. Escaped Parallel tokens are
  observation-only after scope close: requested state remains observable, but close clears the
  parent-only reason, so `throwIfRequested()` has a null reason.
- `sleep()` and `reschedule()` are returned to the implemented kernel section; only combinator
  helpers remain under the explicit **DEFERRED** library heading.

The current complete Parallel target ledger is now exact: prelude **6/6**, execution **20/20**, and
heap **11/11** pass on macOS AArch64, Linux AArch64, and Linux x86_64. The new Async admission and
escaped-token reducers pass on the same three hosts. This amendment invalidates every prior hash and
transcript; freeze a fresh companion hash before starting the next independent three-model round.

## Eleventh exact-hash review precondition — 2026-09-22

GLM 5.3 locked and rejected the Tenth-round candidate
`cf7f92625d306d1afaa0085e058af03182b2e9b89e99134b412366755446f35d`. Its evidence and lifecycle
findings are reconciled as follows:

- The complete current Async matrix is replayed, not inferred: the `async_` filter passes
  **111/111** on macOS AArch64, Linux AArch64, and Linux x86_64. It includes functional scheduler
  behavior, runtime-GC/heap reducers, descriptor/reactor behavior, and the CLI monitor regression.
  The current Parallel matrix remains prelude **6/6**, execution **20/20**, and heap **11/11** on
  those same three executable hosts.
- The acceptance ledger and 2026-09-21 target ledger now explicitly mark prior dated results as
  historical provenance. Their stale Ninth pointer is replaced by this Eleventh current-candidate
  ledger; no historical 34/34 or cancellation claim is used to close the current hash.
- The locked Async diagram now includes cancellation from Created/Runnable states and cancellation
  wakeups from every parked state. It distinguishes direct terminalization of unstarted work from
  cooperative resumption and terminalization of an already-started task.
- `Parallel\run()` explicitly returns the root body's normal value after deliberate group
  cancellation when every child is terminal Cancelled and no genuine worker failure occurred.

This documentation/evidence amendment invalidates every earlier hash and transcript. Freeze a fresh
companion hash, then begin a new complete three-model review round.

## Twelfth exact-hash review precondition — 2026-09-22

GLM 5.3 locked and rejected the Eleventh-round candidate
`0545c13cc0e19fc6db0d30a794f0baaa21f8f05aa783ba42060639ae1d7034a2`. The remaining public rules
are now pinned with executable reducers:

- Parallel has unbounded FIFO submission and bounded admission in v1. Its current seven-worker
  capacity is explicitly an implementation detail, not a public concurrency guarantee.
  `ContextUnavailable` means an admitted worker's configured context-acquire callback returned
  null; it is never saturation/overflow. The public docs, architecture diagram, scheduling policy,
  and `TaskFailureKind` contract agree on those semantics.
- A dynamic nested `Parallel\run()` inside a worker is runtime-rejected as
  `TaskFailureKind::PhpThrowable` with remote class `Error` and its exact nested-scope message. The
  direct `call_user_func` reproducer passes on macOS AArch64, Linux AArch64, and Linux x86_64.
- Awaiting a cancelled Async child, including from the root, throws the scheduler's first-request
  reason. `cancel(null)` remains authoritative and is not replaced by a later cleanup failure; both
  reducers pass on all three executable hosts.
- The complete core matrices remain current evidence: Async **111/111** and Parallel execution
  **20/20**, prelude **6/6**, heap **11/11** pass on macOS AArch64, Linux AArch64, and Linux x86_64.
  The three new source-level reducers are replayed separately on that same matrix after their
  addition, so their coverage is tied to this candidate rather than inferred from an older run.

This amendment invalidates every prior hash and transcript. Freeze a fresh companion hash, then
begin a new complete three-model review round.

## Thirteenth exact-hash review precondition — 2026-09-22

GLM 5.3 locked and rejected the Twelfth-round candidate
`e27aec07b15a51587b7872646002fe52d63fbadca3a5825b500910f563bd8a38` for remaining document
authority gaps. The reconciliation is structural and durable:

- Every historical-evidence pointer now resolves to the final exact-hash precondition immediately
  before the consensus procedure, rather than a numbered precondition that becomes stale after the
  next amendment.
- The EIR operation is named `ParallelSpawn` everywhere in the candidate.
- The earlier Thirteenth candidate's `iff every child is terminal Cancelled` wording is superseded:
  after a normal root return, the actual criterion is that no child failure remains unobserved.
  Successful, Cancelled, and joined-and-caught failed children all permit the root value; only an
  unobserved failed child produces `TaskGroupFailure`.

No runtime behavior changed in this amendment; the current complete Async/Parallel matrices and
tri-target observable reducers remain the evidence for this final candidate. Freeze a fresh
companion hash, then begin a new complete three-model review round.

## Fourteenth exact-hash review precondition — 2026-09-22

Kimi K3 locked and rejected the Thirteenth-round candidate
`7fa019e65e8e329cb25ae6f980b6065f0767e1753b0d35c27b668ecbbeafe386`. The final ownership and
terminology clarifications are now normative:

- Scope drain is the sole normal owner that releases a registered Future's native job and clears its
  `jobId`. An escaped Future destructor releases copied terminal result/failure storage only; its
  native-release branch is defensive for a handle that never reached scope drain.
- A Parallel root failure stores its parent-local drain reason; child-triggered fail-fast has null
  parent/worker reason. First-request-wins prevents any later explicit `cancel(?Throwable)` from
  replacing either state, and no parent user-code turn exists during drain.
- The sole transferable reference-like value is consistently named the Async `Cancellation` token,
  never a `CancelledException`.

No runtime code or target evidence changed in this documentation clarification. The current complete
Async/Parallel matrices and tri-target observable reducers remain authoritative. Freeze a fresh
companion hash, then begin a new complete three-model review round.

## Fifteenth exact-hash review precondition — 2026-09-22

GLM 5.3 locked and rejected the Fourteenth-round candidate
`82c4cf0f2f9f1cc724122ba22c6f5dcb03184ae169db9e120490bbe0f490300d`. Its remaining lifecycle and
composition distinctions are now explicit and replayed on every executable host:

- A request-paired `CancelledException` escaping an Async child is terminal `Cancelled`, preserving
  a normally returning root's value. A caught cancellation followed by `return 7` is instead
  `Completed(7)` and is returned by `await()`. A user-thrown cancellation without an active request
  is an ordinary failure. Both reducers pass on macOS AArch64, Linux AArch64, and Linux x86_64.
- `Awaitable::await()` on a cancelled child, including from the root, rethrows the scheduler's
  first-request reason; an earlier `cancel(null)` remains null despite later cleanup failure. The
  corresponding reducers pass on the same three hosts.
- `Async\run()` in a `Parallel\run()` parent root body is supported only without an ambient Async
  scheduler: it runs in the main context and synchronously returns before the Parallel root
  continues. A parent root invoked from an Async task remains in that task's Fiber, so its further
  Async root is rejected by the ordinary nesting guard; the composition reducer pins this split.

No runtime behavior changed in this clarification. The complete Async **111/111** and Parallel
prelude **6/6**, execution **20/20**, heap **11/11** matrices plus all tri-target incremental
reducers remain the authoritative evidence. Freeze a fresh companion hash, then begin a new
complete three-model review round.

## Sixteenth exact-hash review precondition — 2026-09-22

GLM 5.3 locked and rejected the Fifteenth-round candidate
`4a8a54124d41f358e2d727b073419abcb3d9d342899b7dbd039e9a2b237b4752`. Its last boundary and
composition findings are now explicit, implemented, and covered by an observable reducer:

- At the Fifteenth-round candidate, a raw OS descriptor was described as any non-negative integer.
  Candidate 30 supersedes that historical wording with the currently validated `0..=2147483647`
  C-`int` range; provenance and lifetime remain the caller's responsibility.
- `Async\run()` remains supported inside the main-context parent root body of `Parallel\run()` only
  when no Async scheduler is already active. It synchronously completes its nested cooperative
  scope before the parent root proceeds; the same call from an Async task's Fiber is rejected by
  the ordinary any-Fiber nesting guard.
- A second `Parallel\run()` from that parent root is rejected in v1, including dynamic dispatch,
  with `Error("Elephc\\Parallel\\run(): nested Parallel scopes are not supported in a parent root
  in v1")`. The source prelude and generated AST use one static parent-root guard that is restored
  in `finally`, preventing a reentrant executor and pool-admission deadlock without changing the
  worker-only guard.
- The dynamic parent-root reducer passes on macOS AArch64, Linux AArch64, and Linux x86_64. The
  generated Parallel-prelude structural parity suite remains **6/6** on macOS AArch64. The prior full
  Async **111/111**, Parallel prelude **6/6**, execution **20/20**, and heap **11/11** matrices are
  retained as dated core evidence; the new source-level behavior is covered by its fresh
  tri-target replay rather than inferred from those historical matrices.

This amendment invalidates every earlier hash and transcript. Freeze a fresh companion hash, then
begin a new complete three-model review round.

## Seventeenth exact-hash review precondition — 2026-09-22

GLM 5.3 locked and rejected the Sixteenth-round candidate
`ec67daff6fe519ef57d271f7621395fc52a43306dff6046889a738d34c3864e4`. Every reported ambiguity
is resolved as a current normative contract with an executable or source-level proof:

- **Observed Parallel failure and normal root return:** after a root body returns normally,
  `Parallel\run()` returns its root value exactly when no failed child remains unobserved. A
  successful child, a Cancelled child, or a failed child whose `Future::join()` was caught does not
  block that return. An unobserved failed child produces `TaskGroupFailure`. The existing
  `parallel_joined_failure_is_not_aggregated_again_at_scope_exit` reducer proves the formerly
  disputed observed-failure case.
- **Async → Parallel → Async composition:** an Async root in a Parallel parent root is supported
  only if that parent root began in the main execution context without an active Async scheduler.
  A `Parallel\run()` invoked from an Async task still executes its parent root in that task's
  Fiber; a further `Async\run()` deterministically throws
  `Error("Elephc\\Async\\run() cannot be nested or called from a Fiber")`. The new
  `async_task_parallel_parent_root_cannot_start_another_async_root` reducer pins this split.
- **Async handle construction:** `TaskGroup` and `Awaitable` are syntactically constructible
  implementation-visible prelude classes, not opaque Parallel-style handles. This is intentional
  and no longer implicit: their constructors retain only explicitly supplied references and never
  look up or attach to an ambient Async scope. Therefore a manually constructed group cannot
  cancel, drain, or observe an active group; `async_forged_group_is_isolated_from_the_active_scope`
  proves that isolation. `run()` and an active group's `spawn()` remain the only v1 operations that
  create active work.
- **Rust panic classification:** a caught callback or worker-thread Rust panic is exactly
  `TaskFailureKind::WorkerPanic`; `Infrastructure` is reserved for non-panic executor publication
  or cleanup failures. The bridge maps the worker-panic status to that distinct enum case before
  it can cross the C ABI.
- **Evidence recency:** the exact current tree passes the focused Parallel surface slice **42/42**
  on macOS AArch64, Linux AArch64, and Linux x86_64; this includes the new composition reducer and
  the pre-existing observed-failure reducer. The new forged-group isolation reducer separately
  passes on those same three hosts. The broader Async **111/111** and Parallel heap **11/11**
  matrices remain explicitly dated historical evidence because this amendment changed only
  documentation and codegen tests after the parent-root runtime change; they are not relabelled as
  exact-tree full-suite evidence. The current host for generated Parallel-prelude structural parity
  is macOS AArch64 **6/6**.

This amendment invalidates every earlier hash and transcript. Freeze a fresh companion hash, then
begin a new complete three-model review round.

## Eighteenth exact-hash review precondition — 2026-09-23

GLM 5.3 locked and accepted the Seventeenth-round candidate
`3f099ddbefb55ace113e607eda0330ad3d4f9c0a19808efc66cb2431c6398c01`. Kimi K3 then rejected that
candidate with one root-cancellation finding and three public-boundary questions. The findings were
checked against the source and the contract is now explicit:

- **Async root cancellation:** the source distinguishes root and child terminalization. Only a
  non-root task whose request-paired `CancelledException` escapes becomes terminal Cancelled. The
  root is exempt from automatic cancellation at its own yield points. Awaiting a Cancelled child
  still throws its request-paired `CancelledException` in the root Fiber. If caught, the root may
  return normally; if uncaught, it is recorded as the root's ordinary failure and `Async\run()`
  rethrows the same exception object after cleanup, preserving the first-request reason. The
  `async_root_awaiting_a_child_cancelled_by_its_own_request_reports_terminal_outcome` proves object
  identity/reason; `async_root_is_not_implicitly_cancelled_at_its_own_yield_points` proves a
  cancelling root can reschedule, sleep for zero, and return 42. The six-test `async_root_` slice
  passed on macOS AArch64, Linux AArch64, and Linux x86_64.
- **Parallel TaskGroup construction:** the constructor is private in the PHP source and generated
  AST. Only `Parallel\run()` creates a v1 group; `parallel_task_group_cannot_be_constructed_outside_run`
  is the executable checker regression. The public contract now states this explicitly.
- **Cancellation token transfer positions:** EPV1 supports the job-local cancellation tag, but the
  PHP/EPV codec deliberately extracts only a direct token argument or direct closure capture from
  the serialized scalar/array payload. Nested arrays/unions and task results therefore cannot carry
  a Cancellation object. The checker previously accepted these shapes recursively and let them
  fail later as a generic worker Infrastructure failure. It now rejects nested argument arrays,
  capture arrays, nested results, direct results, and unions containing Cancellation before codegen.
  Direct argument and direct capture (including the named `use (&$cancellation)` form) remain
  supported; a new end-to-end reducer proves a direct argument becomes a worker-local proxy. Three
  classifier unit tests and four focused error reducers pin admission/rejection.
- **Ordinary Fiber composition:** `Parallel\run()` is supported from a non-Async, user-created
  Fiber; the parent body remains in that Fiber while its OS thread blocks through worker drain. The
  ordinary-Fiber reducer, direct Cancellation argument/capture transfer, and the then-current
  focused Parallel surface slice pass **44/44** on macOS AArch64, Linux AArch64, and Linux x86_64.
  The curl multi-transfer case returns early under its native-dependency skip gate on Linux; the
  Seventeenth candidate described the Linux result as 44/44 plus that skip, which the Nineteenth
  reconciliation below clarifies as 43 executed surface cases and one conditional skip. Async-task
  entry remains legal with the established blocking warning, while worker and reentrant parent-root
  entry keep their distinct fail-closed guards.
- **Checker-only gates:** the three `parallel_transfer::tests` classifier unit tests and all four
  focused Cancellation transfer error reducers pass on macOS AArch64; the existing private
  `Parallel\TaskGroup` constructor error test also passes there. These checks are pre-codegen type
  analysis, and the generated Parallel surface itself is covered by the stated three-host matrix.

This amendment changes checker behavior and the locked public transfer/composition contracts. It
invalidates the Seventeenth candidate and all locks on that hash. Freeze a fresh companion SHA-256,
then restart the complete three-review round from GLM.

## Nineteenth exact-hash review precondition — 2026-09-23

GLM 5.3 locked and rejected the Eighteenth-round candidate
`e081086c7cb498e2f1c1395f50d2d955bdc63fd9099e6cc1be2751ea58ff52ae`. Its finding and four questions
are reconciled in the current candidate:

- **Async root failure ordering:** the Eighteenth candidate overgeneralized that an escaping
  request-paired root `CancelledException` always wins. It is a root failure and preserves its
  object/reason only if it is first among unobserved failures in occurrence order. An earlier
  unobserved child failure remains primary under the existing global rule.
  `async_root_cancelled_child_does_not_override_an_earlier_child_failure` reproduces this combined
  case; `async_root_` passes **7/7** on macOS AArch64, Linux AArch64, and Linux x86_64.
- **Parallel test counts:** the Eighteenth candidate's `parallel_` slice had 44 test functions; the
  Cargo summaries reported 44 passed even though Linux's curl multi-transfer test returns early
  through its native-dependency skip gate. The Nineteenth tree added one parent-Fiber suspension
  test, so `parallel_` had **45 functions**: macOS executed all 45; each Linux run
  exercised 44 and had one
  conditional curl skip (Cargo still counts that early return as passed). No executed test fails on
  macOS AArch64, Linux AArch64, or Linux x86_64.
- **Classifier count and host:** the implementation-slice evidence now names all three current
  `parallel_transfer::tests` cases. Those three classifier tests, four focused Cancellation
  transfer error reducers, and `parallel_task_group_cannot_be_constructed_outside_run` were run on
  macOS AArch64; they are pre-codegen type-analysis gates with no target-specific branch. The
  resulting Parallel execution and direct-token argument/capture behavior pass in the three-host
  matrix above.
**Historical clarification:** the immediately following Nineteenth-round Fiber allowance was a
then-current proposal but is superseded by locked Consensus question 21 and is not normative.

- **Suspended parent Fiber (historical, superseded by locked question 21):** an ordinary user Fiber may suspend inside a Parallel parent body.
  The outer scope remains active, submitted jobs continue, and another `Parallel\run()` in the same
  context is rejected with the parent-root nesting error until the original Fiber resumes. Its body
  then completes and the owning scope drains all jobs before returning. The new
  `suspended_parallel_parent_keeps_its_scope_active_until_it_resumes_and_drains` reducer pins this
  behavior and is included in the three-host Parallel slice. GLM's wording that such an overlap was
  undefined is answered by this v1 rule.

**Historical and superseded:** the Nineteenth candidate temporarily allowed user suspension and
resumer-side handle use. That behavior was rejected in the Twentieth review and replaced by the
currently locked Consensus question 21, which rejects direct and dynamic user suspension before
argument evaluation while a Parallel parent or worker scope is active. None of the Nineteenth
handoff prose is normative for v1. At the time, the Nineteenth amendment changed the Parallel/Fiber
lifecycle wording and root cancellation ordering guarantee, superseding the Eighteenth candidate's
unconditional root-exception propagation sentence and ambiguous test-count description.

## Twentieth exact-hash review candidate — 2026-09-23 (historical; rejected; handoff never locked)

The Nineteenth review left one operational question open: while a Parallel parent Fiber is suspended,
can the Fiber resumer use live handles yielded by `Fiber::suspend`, or does the scope merely remain
active without making those handles usable outside the parent Fiber?

The Twentieth proposal said the resumer could use those handles while the owning Parallel root
remained suspended and active. `suspended_parallel_parent_hands_active_handles_to_the_resumer` yielded the live
`TaskGroup` and `Future`; the resumer joins that Future, spawns and joins another worker through the
same TaskGroup, runs an independent Async root, then resumes the original Fiber. The reducer asserts
`42|1|7|42`. This is not TaskGroup escape: the root callback has not returned, so it continues to
own the structured scope. A second Parallel run remains rejected until the
original parent Fiber resumes and the scope drains, as pinned by
`suspended_parallel_parent_keeps_its_scope_active_until_it_resumes_and_drains`.

The resulting evidence is intentionally stated per host and scope: the macOS AArch64 full
`parallel_` slice passes **46/46**; Linux x86_64 Docker reports **46/46** (the curl multi-transfer
case retains its conditional native-dependency early exit); Linux AArch64 Docker runs the two
parent-suspension reducers **2/2**. The handle-handoff behavior therefore has executable evidence on
all three hosts, while the complete current Linux ARM64 `parallel_` slice remains open.

This proposed public Fiber/Parallel composition change was rejected and invalidated the Nineteenth
candidate hash and every review lock on it. The compiler lowering lifetime pin and receiver cleanup
adjustment applied only to the rejected experiment. The subsequently locked v1 contract in question
21 removes external handoff and is authoritative; no Nineteenth/Twentieth acceptance counts.

### Twentieth review outcome — GLM 5.3 rejected hash `df0b8af6b85c4a5e4190acd3d8acb77072d7968247c92114495f3011af3913a1`

GLM found that the resumer used an unsettled `Future` outside `Parallel\\run()`'s dynamic call extent,
which appeared to contradict the locked rule that only settled Futures escape scope drain. It also
found no heap-debug ownership reducer for the new mixed-receiver path. The unanswered questions
were how an abandoned parent Fiber closes its scope, whether raw `Fiber::suspend()` is an Async
scheduler yield, and how resumer-side cancellation and worker failures surface. The Twentieth
handoff proposal is historical only; its verdict was **REJECT** and it did not lock an API.

## Twenty-first candidate pre-review contract — 2026-09-23 (historical; rejected exact-hash round)

The exact-hash Twenty-first review round was rejected for missing callback guards, incomplete Async
yield exemptions, and macOS evidence. Its core API decision—reject every supported user-origin
`Fiber::suspend()` inside a Parallel parent/worker scope before argument evaluation—was nevertheless
explicitly locked as Consensus question 21 and remains the current v1 contract. Later candidates
closed the implementation/evidence findings without restoring the rejected handoff behavior.

The v1 contract keeps the settled-Future escape rule unchanged by removing external handoff from an
active Parallel root. An ordinary Fiber may call `Parallel\\run()` synchronously, but user-level
`Fiber::suspend()` is rejected before argument evaluation while a parent scope is active on the
current OS thread. The same fail-closed guard applies to user suspension from isolated Parallel
workers. This prevents both live `TaskGroup`/unsettled `Future` transfer to a resumer and an
abandoned owner Fiber from stranding the structured scope. A rejected suspend has the exact
diagnostic `Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active
in v1`; its argument expressions are not evaluated.

The parent-scope active state is thread-local in `elephc-parallel`; `Parallel::run()` enters it
through TaskGroup construction and leaves it in the owner scope's `finally` path. Compiler-inserted
guards consult that bit before lowering the user call. Async scheduler-owned `awaitTask()` and
`rescheduleCurrent()` yields are the explicit exception: their runnable/wait state is registered
and the Async scheduler retains and resumes the Fiber before its root returns. Raw user suspension
inside an Async task is not a wake source; if no runnable task or other wake source remains,
`Async\\run()` throws `Error("Elephc Async deadlock: tasks are live but none are runnable")`.

Executable evidence: macOS AArch64 passes the full `parallel_` slice **47/47**; Linux AArch64 passes
the four focused owner-guard, Async-yield, worker-guard, and raw-Async-suspend reducers **4/4**;
Linux x86_64 passes those same focused reducers **4/4**. The bridge's parent-scope TLS unit test
passes on macOS AArch64. Full Linux `parallel_` suites remain open; the evidence here is explicitly
the focused API/lifecycle surface.

This narrows the proposed Fiber/Parallel composition API and invalidates the Twentieth hash and all
reviews on it. Freeze a new companion hash and restart the complete three-model exact-hash review;
no prior acceptance counts for this candidate.

### Twenty-first review outcome — GLM 5.3 rejected hash `2133baba8559ef8f273f1885f5fa0357c162c77b98d1870568d76b84be9ebab5`

GLM verified the TLS enter/leave pairing, direct call argument ordering, direct owner/worker
diagnostics, the `awaitTask()`/`rescheduleCurrent()` composition reducers, and the settled-Future
scope rule. It rejected this candidate on three material omissions:

- **B1 — dynamic callback gap:** only syntactic static calls reached the compiler-inserted guard;
  first-class and runtime-selected `Fiber::suspend` callbacks had no fail-closed check, so they
  could yield a live `TaskGroup`/unsettled `Future`.
- **B2 — incomplete Async exemption:** `sleepCurrent()` and the shared `awaitIo()` suspension site
  were missing from the exemption even though the docs promised `sleep()`, `awaitReadable()`, and
  `awaitWritable()` under a Parallel parent scope. No reducer proved those sites in that composition.
- **B3 — missing host evidence:** the raw Async-suspend deadlock reducer had no macOS AArch64 result
  for this candidate hash.

GLM also requested two clarity repairs: align the numeric drain-phase comment with the five named
phases, and describe the Parallel-in-Async warning as synchronous caller behavior without implying
that explicitly scheduled Async tasks never run during the scope. Its open full-Linux-suite note is
historical only and superseded by the later Candidate-35 full Linux x86_64/AArch64 matrices below.
The final verdict was **REJECT** with unresolved questions;
no review of hash 2133baba... counts toward consensus.

## Twenty-second exact-hash review precondition — 2026-09-23 (historical)

Candidate 22 preserves the settled-Future escape boundary and rejects every supported user-origin
`Fiber::suspend` invocation while a Parallel parent or worker scope is active. Statically known
calls keep the pre-argument guard. Runtime-selected string, callable-array, Mixed, and first-class
descriptor targets are preflighted after evaluating the callback expression but before evaluating
the invocation arguments; matching callbacks raise the same `Error`. The callable-descriptor
preflight uses its recorded PHP-visible target name, and the array preflight is a read-only boolean
predicate before raising, so caught errors retain clean ownership. The preflight covers direct
dynamic invocation, `call_user_func()`, and `call_user_func_array()`.

Async's only trusted Fiber-yield owners are `__Scheduler::awaitTask()`,
`rescheduleCurrent()`, `sleepCurrent()`, and `awaitIo()`; the public `awaitReadable()` and
`awaitWritable()` methods both use `awaitIo()`. User-authored raw suspension remains a deadlock,
not a scheduler wake source. The warning says the proven `Parallel\\run()` call is synchronous
until drain while explicitly acknowledging that scheduler-owned Async yields inside its body can
run tasks; it does not claim implicit offload or preemption.

Executable evidence before review: the complete `parallel_` slice passes **50/50** on macOS AArch64,
Linux AArch64, and Linux x86_64. On both Linux hosts the existing Curl multi-transfer fixture takes
its native-dependency early-return path; this is recorded, not counted as a Curl execution claim.
The additional opaque first-class/Mixed `Fiber::suspend` reducer passes **1/1** on all three hosts.
The raw Async-suspend/deadlock reducer passes **1/1** on macOS AArch64; the full Parallel composition
reducer exercises `awaitTask`, `rescheduleCurrent`, `sleepCurrent`, `awaitIo` via both readable and
writable waits, on each full `parallel_` host. Parallel-prelude source/AST/surface tests pass **6/6**,
the `elephc-parallel` unit package passes **41/41**, and the explicit iOS library-refusal / `--check`
test passes **1/1** for device and Simulator. `cargo build` and targeted `rustfmt --check` are green.

### Twenty-second review outcome — GLM 5.3 rejected hash `f10e36fd9ae89b017cf3dceca15d895ccfec450423a5db8d934b47314db041fd`

GLM verified the direct Fiber guard, Async-owned suspension exemptions, the three-host reducers,
and iOS refusal evidence, but rejected the candidate for an incomplete dynamic-callback boundary
and missing negative-direction coverage. It also questioned whether the candidate-21 drain-phase
comment repair had been recorded. Source inspection confirms that the shipped comment at
`src/parallel_prelude/source.php` explicitly names all five native phases and correctly treats
cancellation as terminal but not failure; the code was already fixed and the candidate-21 review
record was stale on that point.

Candidate 22's acceptance never counted toward consensus. The local Ollama output was truncated
while capturing GLM's long-form response, so this is a finding summary, not a complete transcript.

## Twenty-third candidate review history — 2026-09-24, no consensus recorded

Candidate 23 used specification hash
`ea356ac59420be2df59a8b72f956753688fda2a0c14a5f94d177ffc4f825dfbb`. It addressed candidate 22's
dynamic callback boundary with a descriptor-invoker backstop and preflight checks for known higher-
order callbacks. Kioku records that candidate 23's GLM/Kimi/DeepSeek exact-hash review was still
pending; no complete `LOCK` + `ACCEPT` round was recorded. It was superseded by the subsequent Kimi
candidate-24 callback-source-order finding, so candidate 23 never counted toward consensus.

## Twenty-fourth review outcome — Kimi K3 rejected candidate 24 hash `e557e805769c79603e5ff10483dc18764d2554a5a2a1747f449d1dccbbfa7f29`

Kimi correctly found that the callback preflight for argument-1 callbacks ran before argument 0.
The PHP oracle prints the input's side effect before invoking `Fiber::suspend`; Elephc had skipped
it. Moving the guard to its source-order position exposed an ownership leak on the rejecting path.
Candidate 25 passes earlier owned operands into the compiler-only guard and releases them only on the
active-scope rejection branch. This retains the earlier PHP-visible effects, skips later operands,
and keeps both caught-error and ordinary-callback paths heap-clean.

Kimi also flagged unconditional release after `__rt_str_persist`. The helper's concat-temp fast path
promotes the same allocation in place, so freeing it would invalidate the returned pointer. The
invoker now compares original and persisted pointers and frees only a distinct superseded allocation.
The literal-string example Kimi gave was not itself the alias case (static inputs are duplicated / ignored by
`heap_free_safe`); pointer identity nevertheless closes the real in-place alias edge defensively.

The suggested TaskGroup cleanup-masking issue did not reproduce on source review: the native
`parent_scope_leave()` always zeros the TLS depth before returning, and a valid parent scope cannot
own that root while the thread is a Parallel worker. The normal dynamic-invoker argument owner is
released by its caller after a successful call; the exception boundary releases it only when a throw
skips that caller cleanup. The non-callable callback concern also did not reproduce: target checks
are representation-aware and only the Fiber suspend target reaches the active-scope assertion.

## Twenty-fifth review outcome — Kimi K3 rejected hash `00869eb141c62cd6c6f4d996b1bddfa92699ddd57a94e5e9ecd9448df924d061`

Candidate 25 was rejected for incomplete evidence on supported higher-order callback consumers and
for omitting closure descriptors from the descriptor-name backstop. Source recheck confirmed the
following review concerns were not findings: `__rt_mixed_unbox` returns the documented tag/payload
registers on both ABIs; the name-length scratch register is compared before the nested string call,
not consumed afterward; the callback guard is present whenever a Parallel scope/worker can exist;
callable-array arity is checked before indexing in both source and AST builders; and reverse cleanup
iterates source-order owned temporaries, not ABI stack slots. `uasort()`/`uksort()` currently stop at
unsupported EIR associative-array lowering, so they have no runtime guard path to test in this task.
Candidate 26 closes the supported-surface evidence gaps with reducers for `array_reduce()`,
`array_walk()`, `array_walk_recursive()` using a known first-class signature, and `usort()`, plus a
positive ordinary-closure descriptor regression. The descriptor-name guard now admits the closure
kind before checking its PHP-visible name. The docs specify that `preg_replace_callback()` evaluates
the pattern first and skips the subject only if its callback guard rejects.

## Twenty-sixth review outcome — Kimi K3 rejected hash `1689b10321b492ab636f8dc4680ae5cc6d9264d52600b47899f1d43565a8fb56`

Candidate 26 was rejected for callback-comparison ABI concerns, descriptor-kind coverage, named /
spread callback forms, and higher-order builtin test coverage. Source recheck confirmed the mixed
unbox register contract and that descriptor kind matching completes before any string comparison.
Candidate 27 restores callback name registers after a non-matching comparison, covers the closure
descriptor kind, tests a Mixed string spelling that must pass the first comparison and match the
leading-backslash spelling, and exercises the supported higher-order callback lowerings.

Named no-spread calls now use the shared `CallArgPlan` to evaluate source expressions in order,
perform the callback check at the mapped parameter's source position, and materialize regular
operands by parameter slot. Static spreads are expanded through the shared call-argument helper
before the same source-order guard. A probe of runtime variadic `array_map(...$arrays)` currently
fails in EIR ArrayMap lowering for nested/multi-array element shapes, so it cannot produce an
executable path that bypasses the descriptor backstop. The docs make the `preg_replace_callback()`
pattern/callback/subject order explicit.

## Twenty-seventh review outcome — Kimi K3 rejected hash `0814fe7c2166ccaa989adf664e0647aa418971697e52ec4583f95f9da2bc1dcf`

Kimi's named-argument finding was valid at this hash: the positional fast path guarded the callback,
but named/spread calls fell through to generic lowering. Candidate 28 uses the shared `CallArgPlan`
for no-spread named calls, including reordered source arguments, and expands statically known spreads
through the shared helper before guarding. Tests pin both callback-first and input-first named
`array_map()` calls plus a statically expanded spread. The dynamic variadic `array_map(...$arrays)`
case is not supported by current EIR ArrayMap lowering (nested/multi-array shapes fail before an
executable is produced); the descriptor-invoker remains the final protection for callback targets
that do reach runtime.

The apparent argument-container leak in the Rethrow invoker path did not reproduce: both public
invoker entry points install an exception boundary, and the boxed argument array is spilled before
the descriptor guard. The direct owned first-class callback is now covered by a heap-debug reducer.
The register findings were also not defects on source inspection: the `x9` candidate-length compare
precedes `__rt_strcasecmp`, and the kind check completes before the candidate-name calls. The
Mixed-unbox contract returns the tag/payload triple on documented ABI registers. Candidate 28
restores the callback name pair on each mismatch edge and tests the Mixed `\\Fiber::suspend` spelling,
which must miss the first candidate and match the second. Pending releases use source-order SSA values
with an explicit ownership proof, not ABI stack slots; the active-state branch runs before any release.

The second source-order callback reducer exposed an ownership issue for captured/local values. The
guard now releases only expression values proven to be owning temporaries; ordinary local/ref-bound
loads remain owned by their slots. Regressions verify dynamic first-class expressions, closure-kind
descriptors, named and statically spread `array_map` calls, and heap-clean rejection.

## Twenty-eighth exact-hash review outcome — Kimi K3 rejected hash `556b964207c750f5cef0bf7d1af7acd9ada5a31e36ba9ce3663a49cc03560ec1`

Candidate 28 retains the descriptor-invoker `Fiber::suspend` backstop for all callable consumers,
including closure descriptors. Supported native higher-order callbacks are guarded at their PHP
argument position. No-spread named calls use `CallArgPlan`; positional calls lower in source order,
and static spreads are expanded through the shared argument helper. Earlier expressions are
evaluated once, only proven owning temporaries are released on the active-scope rejection edge, and
later operands are skipped. Local/ref-bound values stay owned by their slots. `array_map()` checks
its callback at argument 0; `array_filter`, `array_reduce`, `array_walk`, `array_walk_recursive`, and
`usort` check at argument 1 after their input. `uasort()`/`uksort()` do not currently reach runtime
because the EIR backend rejects their associative-array lowering. `preg_replace_callback()` lowers
pattern, callback, and subject once in source order and checks at argument 1; rejection evaluates the
pattern but skips the subject. When no Parallel scope/worker is active, the guard leaves argument
ownership unchanged.

Concat-string callback results are persisted before restoring the caller's scratch offset. If
`__rt_str_persist` takes over a concat-temporary block in place, pointer equality prevents freeing
the result; distinct superseded owned strings are still released.

Regressions cover named and positional calls, statically expanded spreads, callback factories,
callbacks through `array_map`, `array_filter`, `array_reduce`, `array_walk`, `array_walk_recursive`,
`usort`, and `preg_replace_callback`, source-order input/pattern effects, skipped later operands,
heap-clean rejection, ordinary callbacks and closure descriptors, Fiber-object callable arrays,
Mixed and leading-backslash strings, and dynamic callable string returns. The four trusted Async
yield owners remain exactly `awaitTask()`, `rescheduleCurrent()`, `sleepCurrent()`, and `awaitIo()`.

Kimi's candidate-28 review also asked whether Async-only runtime-resolved callbacks fail closed without
the Parallel bridge. A new Async-only `array_map()` reproducer confirmed the gap as a SIGSEGV.
Candidate 28 is rejected; its other ABI/ownership questions were source-rechecked, but only the
reproduced Async-only finding is carried into candidate 29 below.

Candidate 26's full 54-test `parallel_` slice passed on macOS AArch64, Linux AArch64, and Linux
x86_64. Candidate 28 has a fresh 54/54 macOS run and focused `parallel_parent_scope_` **5/5** runs on
all three executable hosts. Parallel-prelude source/AST/surface tests pass **6/6**, and the iOS
Parallel refusal / `--check` reducer passes **1/1**. The candidate-28 precondition originally called
for all three exact-hash reviews; Kimi K3 rejected candidate 28, so no consensus was recorded and no
later reviewer acceptance on that hash counts.

## Twenty-ninth exact-hash review precondition — 2026-09-24 (historical, rejected below)

Candidate 29 closes an Async-only crash found after candidate 28: a runtime-resolved
`Fiber::suspend` callback invoked by a supported higher-order builtin segfaulted when the Parallel
bridge was absent, because the callback preflight was emitted only for Parallel programs. Each call
site now gets exactly one preflight for string, callable-array, Mixed, and first-class descriptor
callback shapes. With Parallel linked, its callback-specific guard checks parent/worker scope first,
then checks whether an Async task is executing; without Parallel, the Async callback guard performs
that check. A matching callback raises `Fiber::suspend() is not a scheduler wakeup inside an Elephc
Async task` before dispatch in an Async task, while ordinary callbacks are unchanged. The direct
Parallel-scope guard remains limited to Parallel parent/worker state, so Async scheduler-owned yields
inside Parallel remain allowed. A scalar private scheduler flag marks the interval around each task
Fiber's `start()` or `resume()` and resets on both normal and caught-Throwable paths. It deliberately
does not retain a Fiber object in PHP static storage, preserving Parallel's heap-clean baseline when
Async declarations are also injected. Raw direct static Async suspension retains its existing
fail-closed deadlock behavior.

Candidate 29 has complete macOS evidence: `cargo build`; Async/Parallel source-AST parity; the full
`codegen::async_scheduler` slice **51/51**; and the full `parallel_` slice **56/56**, including every
Parallel heap-clean regression. The dynamic-string, callable-array, ordinary-callback, and combined
Async+Parallel callback guards each have focused passing reducers. The same Linux executable filters
pass **8/8 on x86_64** and **8/8 on AArch64**: five `parallel_parent_scope_` regressions, the combined
Async+Parallel callback regression, and both Async-only runtime callback regressions. Candidate 28
previously exposed an x86-only false-positive regression while two preflights were emitted; candidate
29 emits only the Parallel preflight in combined programs and consults Async activity only in its
callback-specific path. Its exact-hash precondition awaited Kimi, GLM, and DeepSeek, but Kimi rejected
the hash and the other two did not review it; no candidate-29 transcript counts toward consensus.

## Twenty-ninth exact-hash review outcome — Kimi K3 rejected hash `7ff9a71b757fd4b1c399fecc4330ab4d75e838e997ce94383abd9c15013c1c69`

Kimi reviewed the complete candidate-29 text and rejected it. Its material findings/questions were
reconciled into candidate 30: the locked-seven-worker bullet conflicted with the later
implementation-only pool-count statement; negative raw descriptors and non-finite/overflowing
timer/timeout values needed exact fail-fast behavior and tests; the current-candidate history skipped
candidate 23; the by-reference Async `Cancellation` exception needed to be explicit beside the
general reference-capture rejection; the running-task cancellation diagram and candidate-29 Async
active-flag scope needed clarification; and the inherited Linux full-matrix gate needed an explicit
result. Candidate 30 records the chosen bounded-admission (not fixed-capacity) rule, negative and
non-finite input errors, the sole by-reference token carve-out, the running cancellation edges,
candidate-23's pending/no-consensus provenance, and full Linux slice evidence. Candidate 29 did not
count toward consensus.

## Thirtieth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 30 carries forward candidate 29's one-preflight rule and additionally makes I/O/timer edge
behavior explicit. `sleep()` and descriptor timeouts reject negative values, `NAN`, infinity, and
finite durations whose nanosecond deadlines cannot fit the signed monotonic clock, before timer or
descriptor registration. Stream resources and integer descriptors in `0..=2147483647` are the
accepted source forms; negative or out-of-range raw descriptors raise `ValueError` before
duplication/`poll()`, and all other source types raise `TypeError`. The descriptor `ValueError`
message identifies `awaitReadable()` or `awaitWritable()`. Regressions cover negative/oversized
descriptors, `NAN`, `INF`, negative timeouts, huge finite deadlines, zero-probe, and invalid source.

The Parallel contract locks bounded admission and unbounded FIFO submission, but not a fixed worker
count. Seven workers are the current implementation's eight-slot pool after reserving the main
context; that capacity may change without changing the v1 API. Transfer rules reject ordinary
by-reference captures but explicitly carve out the direct-argument/direct-capture Async
`Cancellation` token, reconstructed as a worker-local proxy. The cancellation section specifies that
an escaping task Throwable supplies a reason only when it initiates the first request; an earlier
explicit `cancel(null)` keeps a null reason. The lifecycle diagram states that the root Fiber is a
task Fiber, running PHP is not preempted, and caught versus escaping request-paired cancellation has
distinct completion states.

Candidate 30 has passing macOS evidence: `cargo build`, ASM-comment alignment, source/AST parity,
Async **53/53**, Parallel **56/56** including heap-clean reducers, plus focused invalid I/O/timer
input tests. Full Linux x86_64 and Linux AArch64 slices both pass: `parallel_` **56/56** and
`codegen::async_scheduler` **53/53** on each target. This closes the carried cross-target acceptance
gate, not merely the earlier 8/8 callback/I/O subset. Kimi K3, GLM 5.3, and DeepSeek must each review the
complete specification at this exact hash and return final `LOCK` + `ACCEPT` transcripts with no
finding and no question. Kimi K3 must re-review the revised candidate after its prior rejection.
Any material finding, question, truncated transcript, or hash mismatch invalidates the round.

## Thirtieth exact-hash review outcome — Kimi K3 rejected hash `877b7947c8acfa25c8c390c5b65da048073a2f4ba226c1c94456171739bebd32`

Kimi returned a complete review with four contract findings and one gate reminder; candidate 30 is
rejected and cannot count as a lock. F-001 asked whether a blocking `Future::join()` can strand an
Async scope after a concurrent failure/cancellation. The source contract shows that blocking join
does prevent the same OS thread's Async scheduler from advancing; v1 does not promise interruption,
and the scope remains blocked until join returns. No Async task can concurrently originate a failure
or cancellation on that same `_rt_ctx`; the warning and blocking behavior are explicit. Candidate 31
states this limit, not a new cancellation mechanism. F-002 asked about abandoning a Parallel owner
Fiber: v1 rejects user suspension while the scope is active, catches a root Throwable, drains and
closes before rethrow, and has a TaskGroup destructor drain fallback. A new reducer pins Fiber-root
exception cleanup. F-003 noted the Async/Parallel constructor asymmetry; candidate 31 records that
Async construction only stores explicit same-context references, while Parallel construction enters
thread-local parent-scope state and must stay private. F-004 requested exact join/aggregation
semantics; candidate 31 states that the group retains each joined Future through drain, successful
values are never aggregated, and already-observed failures are excluded. F-005 restated the existing
consensus gate; the checklist and candidate-31 precondition already leave all three exact-hash locks
pending. No transcript from candidate 30 counts toward consensus.

Additional questions in the earlier candidate-30 Kimi transcript are also resolved in candidate 31:
while the scope is live, terminal Awaitables may be observed outside a task but pending ones throw
the existing task-context Error; all Awaitables expire when `run()` closes their scheduler. Repeated
cancellation while a group is open is a no-op, but closed groups reject it; negative IEEE-754 zero
follows the zero-duration reschedule/probe path. Focused regressions cover post-scope Awaitable
invalidation, repeated first-request cancellation, signed-zero sleep, and Fiber-root scope drain.

## Thirty-first exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 31 adds the clarifications and focused regressions listed in the preceding outcome. The
runtime now marks the Async scheduler closed at the start of teardown; every escaped Awaitable
operation then fails with the specific `Elephc Async Awaitable scope has ended` error instead of
falling through to the generic stale-task-handle error. `build_expr.in` and `source.php` remain in
AST/source parity. Other key semantics are backed by the first-request cancellation guard, finite
and descriptor-range checks, and `src/parallel_prelude/source.php`'s scope-owned Future drain and
`run()` cleanup path. Focused reducers are `async_completed_awaitable_expires_after_scope_close`
(terminal handle expiry), `async_awaiting_a_cancelled_child_preserves_the_first_request_reason`
(repeated Async cancellation), the expanded `async_root_is_not_implicitly_cancelled_at_its_own_yield_points`
(signed-zero sleep), `parallel_group_cancel_requests_workers_without_transferring_parent_reason`
(repeated Parallel cancellation), and `parallel_scope_drains_before_a_fiber_root_exception_escapes`
(owner Fiber cleanup). Candidate-31 focused evidence passes: macOS `cargo build`, rustfmt and
`git diff --check`; Async prelude PHP-oracle/source-AST parity; the Awaitable-expiry, Async
first-request/signed-zero, Parallel first-request, and Fiber-root cleanup reducers on macOS; Linux
x86_64 `async_completed_awaitable_expires_after_scope_close` **1/1**; and Linux AArch64
`parallel_scope_drains_before_a_fiber_root_exception_escapes` **1/1** and
`async_completed_awaitable_expires_after_scope_close` **1/1**. The full macOS and Linux
Async/Parallel slices recorded above belong to candidate 30 and are retained as the broader carried
matrix; candidate 31's new runtime delta is covered by the focused tests listed here. The final
candidate hash must be recomputed after source and documentation review. Kimi K3, GLM 5.3, and
DeepSeek must each review the complete specification at the same exact hash and return a complete
`LOCK` + `ACCEPT` with no findings or questions. No reviewer can count before the final hash is
frozen, and Kimi must review again after its candidate-30 rejection.

## Thirty-first exact-hash review outcome — Kimi K3 rejected hash `a83fcfe5b809c48bf7b3d7598f232679d12943e6558863e2fc22e66d7caf182d`

Kimi returned a complete review with seven findings and five questions; candidate 31 is rejected.
F-001's root-Async-versus-Parallel reason concern is resolved by stating explicitly that Async's
request-paired root exception and retained first-request reason are in a separate PHP state and are
not cleared by Parallel's parent-only reason cleanup. F-002 is source-verified: the job cancellation
bit is written/read under the same native mutex. Candidate 32 defines the per-job linearization and
cooperative race: a post-return observation of a still-running job sees the bit; a worker that passed
its checkpoint before the request is not preempted, and already terminal jobs retain their outcome.
F-003 is resolved by stating that `Future::join()` leaves Async timer deadlines and duplicated I/O
registrations owned/armed but the scheduler does not poll or dispatch until the blocking call
returns; deadlines continue to elapse. F-004 is clarified: Async tokens retain ordinary PHP object
references in the same live runtime context; scheduler teardown does not destroy that context or
leave an arena pointer. F-005 is clarified: a worker gets a fresh proxy object with no `===` identity
preservation, and a Cancellation result cannot cross back. F-006's missing AArch64 expiry test was
run and passes **1/1**. F-007 is fixed in candidate 32: `Parallel\run()` now calls the idempotent
cancel/drain/close helper from its `finally` even when another userland reference retains the group;
the destructor uses the same helper as fallback. The retained-group root-exception reducer verifies
closed-state behavior and no leaked parent scope. No candidate-31 transcript counts toward consensus.

## Thirty-second exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 32 adds those clarifications and changes Parallel scope cleanup so the last-reference PHP
destructor is no longer the sole fallback. `TaskGroup::__cleanupOnExit()` is a private,
compiler-whitelisted helper used by both `__destruct()` and `Parallel\run()`'s `finally`; it
idempotently cancels and drains if necessary, closes retained handles, and releases the parent-scope
guard even if close itself throws. `__close()` marks the capability closed before releasing owned
Future/token references, so destructor reentry cannot see a half-closed live group. A user-retained
group therefore cannot outlive `run()` as an open capability. The checker whitelist keeps this
helper inaccessible to user calls. A new regression
`parallel_task_group_retained_by_caller_is_closed_when_root_throws` pins post-scope `Error` and a
subsequent successful scope; `parallel_internal_bookkeeping_authority_is_private_to_the_scope` now
also proves the helper remains private. MacOS focused evidence passes: `cargo build`, rustfmt,
`git diff --check`, Parallel prelude source/AST/PHP-oracle parity, both new/extended codegen reducers,
and the established Async expiry/cancellation/signed-zero/Fiber-cleanup reducers. Linux x86_64 and
Linux AArch64 each pass `parallel_task_group_retained_by_caller_is_closed_when_root_throws` **1/1**
on the current code. Candidate 31's Linux Async-awaitable expiry reducer passed **1/1** on both
x86_64 and AArch64. The broad full Async/Parallel matrices remain the candidate-30 results recorded
above; candidate 32's new cleanup delta has its own focused tri-target reducer.

Kimi K3, GLM 5.3, and DeepSeek were to review the entire specification at the same frozen candidate
32 hash. Kimi rejected that hash as recorded below; GLM and DeepSeek did not review it and no part of
the candidate-32 round counts toward consensus.

## Thirty-second exact-hash review outcome — Kimi K3 rejected hash `d6de91c2d1dd9e5d4ff561361851499f5b03f7c72d1d03e44ba762cc074f06c3`

Kimi returned a complete review with four findings and four questions; candidate 32 is rejected.
F-001/Q001 observed that the Nineteenth/Twentieth historical Fiber handoff proposal contradicted
locked question 21. Candidate 33 marks those passages as historical/rejected, states that the exact
Q21 direct/dynamic suspend rejection is current, and identifies Q21's later implementation fixes as
preserving that rule. F-002/Q003 are resolved by narrowing Q16 explicitly to `TaskGroup::spawn()`
worker callback call graphs in isolated contexts; Q10's parent body and same-context Async siblings
may run at explicit scheduler yields and retain ordinary PHP global/static semantics. F-003/Q002
states that spawning after cancellation is rejected before job creation; previously admitted jobs see
the native requested bit in fresh worker proxies, but the parent's Throwable reason is never copied,
so worker `reason()` remains null. The same mutex linearizes request writes and observations; a worker
that checked before the request is not preempted, and terminal outcomes that win publication remain
terminal. F-004/Q004 expands the lifecycle diagram: an observed request-paired exception may escape
to terminal Cancelled, be caught and continue to Completed/Failed, while an unrequested user-thrown
`CancelledException` is Failed; the root remains an ordinary failure rather than a child Cancelled
phase. All four findings and questions are explicit in candidate 33. No candidate-32 transcript
counts toward consensus.

## Thirty-third exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 33 is a specification clarification only; candidate-32 runtime and focused tri-target
evidence remain unchanged. It labels the rejected Nineteenth/Twentieth Fiber-handoff language as
historical, confines Q16's global/static restriction to isolated worker callbacks, defines the
post-cancel observation and reason contract, and completes the Async cancellation lifecycle diagram.
The exact Kimi review was REJECT on candidate 32; GLM 5.3 and DeepSeek have not reviewed candidate 33.
Freeze the final hash after this text is reconciled, then give Kimi K3, GLM 5.3, and DeepSeek the
same complete document and require complete exact-hash `LOCK` + `ACCEPT` transcripts with no finding
or question. Any material change restarts all three reviews at the new hash.

## Thirty-third exact-hash review outcome — Kimi K3 rejected hash `e093e329c18017f599f9338ba37cdcc68b1c118617aef178c0b0c7c957fd4c56`

Kimi returned four findings and four questions; candidate 33 is rejected. F-001/Q001 are resolved by
explicitly marking the Nineteenth/Twentieth allowance for parent-Fiber suspension and live-handle
handoff as historical and rejected. Consensus question 21's reject-before-argument-evaluation rule
is the current v1 contract; the Twenty-first hash itself was rejected for implementation/evidence
gaps but its API decision was later locked and implemented. F-002/Q003 are resolved by clarifying
that question 16's global/static restriction applies only to call graphs executed inside isolated
Parallel worker callbacks. The parent closure and same-context Async tasks at explicit scheduler
yields are outside that worker-only rule. F-003/Q002 is answered: no worker may be created after a
group cancellation request because `spawn()` then throws before job creation; already-admitted worker
proxies observe only their native request bit and never receive a parent Throwable reason. Mutex
linearization and terminal outcomes are stated in the cancellation contract. F-004/Q004 is fixed in
the diagram: observed request-paired exceptions can be caught and continue to Completed/Failed or
escape a child to Cancelled; a user-thrown unpaired `CancelledException` is Failed; the root has its
ordinary failure phase rather than the child-only Cancelled phase. Candidate 33's exact Kimi response
was REJECT with completeness `incomplete`; no candidate-33 transcript counts toward consensus.

## Thirty-fourth exact-hash review precondition — 2026-09-25 (historical)

Candidate 34 contained the contract/history clarifications over candidate 32. Its Linux `parallel_`
slice passed **58/58** on x86_64 and **58/58** on Linux AArch64; the Async Awaitable-expiry delta
passed **1/1** on macOS and both Linux architectures. Kimi K3 returned a complete exact-hash
`ACCEPT` at `0aa15e3c106b84566e8c7fb2090dde08eb3b519459cfd1f56b27df819c1d9ddc`. The attempted GLM
review emitted long analysis without a final structured verdict and is incomplete; DeepSeek did not
review candidate 34. Thus candidate 34 never reached consensus.

After Kimi's acceptance, a full macOS `codegen::async_scheduler` run found one real bug: the new
scheduler `closed` flag caused `async_task_generation_rejects_a_stale_handle_after_slot_reuse` to
report `Elephc Async Awaitable scope has ended` rather than `Stale Elephc Async task handle` when the
internal scheduler was reused for a later root. The exact-tree run was **53/54**, with that one
failure; candidate 34 is therefore rejected as an implementation state despite Kimi's specification
acceptance.

## Thirty-fifth exact-hash review precondition — 2026-09-25 (superseded before review)

Candidate 35 fixes the reproduced stale-handle regression. `runRoot()` reopens the private scheduler
for an internal new root while preserving its monotonic task-generation counter; an old handle then
fails generation validation as `Stale`. Public `Async\run()` still creates a fresh scheduler, so an
escaped Awaitable after its closed scope continues to fail with
`Elephc Async Awaitable scope has ended`. The PHP source and AST builder both set the state, and the
source comment now distinguishes root-scope teardown from internal scheduler-object reuse.

Candidate-35 current-tree evidence passes on all executable hosts: macOS **54/54**
`codegen::async_scheduler` and **58/58** `parallel_`; Linux x86_64 **54/54** Async and **58/58**
Parallel; Linux AArch64 **54/54** Async and **58/58** Parallel. The stale-generation and closed-scope
Awaitable reducers pass **1/1** on macOS and both Linux architectures. `cargo build`, Async
source/AST/PHP-oracle parity, Parallel source/AST/PHP-oracle parity, rustfmt, and `git diff --check`
also pass. Kimi K3, GLM 5.3, and DeepSeek must each review the complete candidate-35 document at the
same frozen hash and return a complete exact-hash `LOCK` + `ACCEPT` with no finding or question. The
Candidate 35 was not submitted as a complete three-model round before further specification
clarifications were made; the candidate-34 Kimi acceptance does not count for it, the prior GLM
transcript is incomplete, and DeepSeek had not reviewed its hash.

## Thirty-sixth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 36 clarifies three candidate-35 review concerns without changing the implementation:
structured scopes guarantee cleanup ordering at exit but not progress under non-returning blocking
calls; `run()` creates the Async/Parallel root while `TaskGroup::spawn()` creates only descendants;
and bounded Parallel admission is normative while its numeric capacity is not. A PHP object alias to
a Parallel TaskGroup may be retained by side effect but is closed in `run()`'s `finally`, so no active
scope authority escapes. Blocking `Future::join()` has no deadlock-free/bounded-progress guarantee
and does not run asynchronous cleanup while blocked. Current code evidence is unchanged from the
Candidate-35 matrices: Async **54/54** and Parallel **58/58** on macOS AArch64, Linux x86_64, and
Linux AArch64; stale-generation and closed-scope Awaitable reducers pass **1/1** on all hosts.
Kimi K3, GLM 5.3, and DeepSeek were to review the complete candidate-36 document at one frozen hash.
Kimi rejected that hash below; no candidate-36 review counts toward consensus.

## Thirty-sixth exact-hash review outcome — Kimi K3 rejected hash `b2376bf3d63aea1c01546126b03618459a158c315db946754ef023af4fa09497`

Kimi returned a complete review with three findings/questions; candidate 36 is rejected. F-001/Q001
noted that the checked checklist item for three independent reviews did not identify that it referred
only to the first frozen hash. Candidate 37 now labels it historical and explicitly states it does
not satisfy later exact-hash rounds. F-002/Q002 identified ambiguity for started tasks already in
the ready queue when cancellation arrives. Candidate 37 distinguishes never-started tasks (cancelled
before first activation without invoking the body) from started runnable tasks (resumed at their
recorded safe point, where cancellation is observed and user cleanup can catch/escape). F-003/Q003
asked how the `use (&$cancellation)` carve-out relates to the general prohibition on worker
by-reference captures. Candidate 37 states in Q16 that the global/static storage rule is separate
from Q19's sole type-specific exception: direct Async `Cancellation` argument/capture is allowed;
all ordinary by-reference captures remain rejected. No candidate-36 transcript counts toward
consensus.

## Thirty-seventh exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 37 incorporated the rejected Candidate-36 clarifications over Candidate-35's already-tested
runtime. It narrowed the first-round review checklist item to its original hash, separated queued
never-started cancellation from resumption of a started runnable Fiber, and explicitly reconciled
Q16/Q19 reference boundaries. No implementation or tests changed. Candidate-35 full current-tree evidence
remains: Async **54/54** and Parallel **58/58** on macOS AArch64, Linux x86_64, and Linux AArch64,
plus stale-generation and closed-scope Awaitable reducers **1/1** on all three hosts. Kimi K3, GLM
5.3, and DeepSeek must each review this complete specification at one identical frozen hash and
return a complete `ACCEPT` with no finding or question; all previous hashes are invalid for this
round. Any material edit restarts all three reviewers at the new hash. Kimi K3 reviewed hash
`eda39b21e9bd0edd5cfa946c0c76a2208a786e327f78181bd15f42b440de58f5` and rejected it. Its lineage
finding is resolved in Candidate 38 below; its proposed cross-domain cancellation question is
answered explicitly above: Async reasons remain in the shared Async context, while Parallel workers
always observe a native cancellation bit and a null local reason.

## Thirty-eighth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 38 incorporates Candidate 36's clarifications, resolves the Candidate-37 Kimi lineage
finding, labels the completed first review cycle inline as historical, and explicitly excludes
Async Throwable reasons from Parallel worker observation. Runtime code and tests remain unchanged
from Candidate 35; its cross-target evidence is recorded in the preceding precondition. All three
independent reviewers must now review the complete specification at one identical frozen hash and
return a complete `ACCEPT` with no finding or question. Any material edit restarts the round. Kimi K3
reviewed hash `0d9bfe9ceabf5a7e0daade0cbb7eb12a85c6d83e16983a23c0f19c5cbc923ead` and rejected it.
Its started-runnable diagram request, Awaitable teardown boundary, and first-request ordering question
are addressed in Candidate 39 below. Its caught-then-different-Throwable and Linux-suite findings
were already answered by the explicit lifecycle outcome paragraph and later Candidate-35 matrices;
those clarifications are made more prominent below. The Async/Parallel cross-domain reason question
was already answered explicitly and remains unchanged.

## Thirty-ninth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 39 makes the started-runnable cancellation transition explicit in the lifecycle diagram,
pins Awaitable invalidation to the first instruction of teardown after the root/children have drained,
and defines Async request ordering at the synchronous explicit call or executor's caught-failure
boundary. It labels the older Candidate-21 full-Linux gap as historical and superseded by the
Candidate-35 full cross-target suite evidence. Runtime code and tests remain unchanged from Candidate
35. All three independent reviewers must review this full specification at one identical frozen hash
and return complete `ACCEPT` verdicts with no findings or questions. Any material edit restarts the
round. Kimi K3 reviewed hash
`049364ac8bc41d0fe10074edee58e35b8a7669c01e4f231ddb88e185e4a32284` and rejected it. The stale
Candidate-37 status, companion-hash authority, and need for one prominent current evidence matrix
are corrected in Candidate 40. Kimi's lifecycle-diagram finding is not accepted as valid: the quoted
`Runnable (already started) -- pending request --> Running at saved point` edge already appears in
the diagram immediately before the cancellation-observation edges. Its caught-then-different-Throwable
and cross-domain cancellation concerns are also answered explicitly in the lifecycle and cancellation
sections.

## Fortieth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 40 updates the status header to the current candidate, explicitly makes the companion
checksum the non-self-referential hash authority, and restates the complete Candidate-35 acceptance
matrix as the latest complete end-to-end evidence for the implemented Candidate-35 runtime baseline:
macOS AArch64 Async **54/54** and
`parallel_` **58/58**; Linux x86_64 Async **54/54** and `parallel_` **58/58**; Linux AArch64 Async
**54/54** and `parallel_` **58/58**. The stale-generation and closed-scope Awaitable reducers each
pass **1/1** on all three executable hosts. Older counts remain valid only for their dated historical
candidate slices and are not evidence for current acceptance. This matrix proves only the runtime
behaviors and test cases present at Candidate 35; every later normative design addition remains
unverified until its implementation and focused regression tests are added. Runtime code and tests
remain unchanged from Candidate 35. All three independent reviewers must review this full specification at
one identical frozen hash and return complete `ACCEPT` verdicts with no findings or questions. Any
material edit restarts the round. Kimi K3 reviewed hash
`1c12bb73a14f7ce6d46f0c7c743df2b0175a0aee0c4e26963f8c8f13e350471c` and rejected it: its finding
identified ambiguity between general wait-point cancellation and root-task exemption. Candidate 41
now explicitly limits cancellation wake/delivery to non-root tasks and states that a root remains
parked at sleep/I/O until its ordinary wakeup.

## Forty-first exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 41 resolves Kimi's root-cancellation finding by explicitly excluding the root task from
group-cancellation wakeups at its own scheduler wait points; a root at sleep/I/O continues only after
its normal wake event and receives no injected cancellation exception. It retains the already-pinned
child-await behavior, in which a root awaiting a terminal cancelled child receives the request-paired
exception. No runtime code or tests changed; Candidate-35 matrices remain the authoritative evidence.
All three independent reviewers must review the full specification at one identical frozen hash and
return complete `ACCEPT` verdicts with no findings or questions. Any material edit restarts the round.
Kimi K3 reviewed hash `ba6365ab34bae501f47861c417c25917813c10412463a3cad529d2a4810f6950` and rejected
it. Its root wait-edge concern is resolved by labeling the lifecycle edge non-root-only and stating
the same exclusion beside the cancellation rules. Its teardown-concurrency concern is resolved by
the single-thread/reentrancy clarification: teardown marks closed before releases, so any reentrant
Awaitable call deterministically throws the scope-ended Error. Its `Future::join()` and post-cancellation
`spawn()` questions are pinned above. Its test-count concern is not accepted as a contradiction:
dated candidate-specific rows describe different test slices, while Candidate 40 names the sole
authoritative current matrix.

## Forty-second exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 42 labels the Async lifecycle cancellation-wake edge non-root-only, states the root's wait
registrations stay active until their normal wake event, clarifies that teardown marks the scope
closed before any releases and that any reentrant access then errors deterministically, and pins the
exact immediate `spawn()` errors and `Future::join()` cancellation deferral. Runtime code and tests
remain unchanged from Candidate 35; the Candidate-40 evidence matrix remains authoritative. All three
independent reviewers must review the complete specification at one identical frozen hash and return
complete `ACCEPT` verdicts with no findings or questions. Any material edit restarts the round. Kimi
K3 reviewed hash `4ec6ea3e3e4a375081ecb60504c151a325d9f95df5cd45602d9924202d44f55a` and rejected it.
Its task-local failure-observation question is resolved by stating the observed bit belongs to the
shared task outcome. Its scope-teardown, join-return, and admission races are pinned above. The new
Candidate-43 lifecycle text says a queued cancellation is only recorded immediately; the task runs
again at its next FIFO selection, while a currently Running task remains so until its next explicit
cancellation point. Outside-task Awaitable `pending` is now defined as every non-terminal outcome.
V1 has no backpressure or queue cap and sustained submissions without worker progress may exhaust
memory. A closed Parallel TaskGroup's exact guard throws the documented `Error`; the source confirms
that same guard is used by the finally cleanup path. A dequeued job remains `Queued` until its atomic
mutex-protected `mark_running()` transition; cancellation wins or loses that one transition. Root
failures are separate from the child-only submission-ordered `failures()` array. Scheduler-owned
duplicated descriptors are not exposed and follow `poll()` error/hangup behavior if externally
invalidated. The iOS refusal is compile-time AOT artifact refusal. Seven worker slots are explicitly
an implementation snapshot, not normative API. The question about `Parallel\TaskGroup` reflection is
answered at the API boundary: direct or dynamic construction follows private-constructor visibility
and cannot create an active scope; constructor-bypass reflection is not a supported scope-entry API.
Its Cancellation proxy identity question is resolved by the existing rule that every allowed worker
capture receives a fresh proxy over the same native request bit, never a PHP reference to parent
storage. Submission order is the order synchronous spawn calls create/reserve monotonically
increasing native job IDs; worker admission/completion order does not redefine it. Worker capacity
remains intentionally opaque and has no v1 runtime query. The macOS-only C-host fault-injection rows
are supplemental diagnostic evidence and are not represented as Linux coverage or as part of the
three-host executable matrix; their target-specific acceptance remains separately open where the
ledger says so.

## Forty-third exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 43 replaces the ambiguous Runnable node labels, defines the `Queued -> Running` mutex
transition as the Parallel admission/cancellation linearization point, makes canceled queued work
skip context acquisition and release any raced no-op thread slot, pins the task-global observed-failure
bit, the exact `Future::join()` post-return behavior, stream-resource duplication/close behavior, and
the stable spawn-order definition of failure aggregation. It separately labels macOS-only C-host
fault-injection observations as supplemental and not cross-target evidence; the Candidate-40 three-host
matrix remains the sole current end-to-end scheduler matrix. Runtime code and tests remain unchanged
from Candidate 35. All three independent reviewers must review the full specification at one identical
frozen hash and return complete `ACCEPT` verdicts with no findings or questions. Any material edit
restarts the round. Kimi K3 reviewed hash
`5c6310398eb864ae8fb124fde2c57f453d466375746a1a5475b2fb7c2ee21661` and rejected it. Its FIFO
selection and Running-task deferred-cancellation points are made explicit in the lifecycle diagram;
the task must be selected by the single-threaded scheduler before it resumes. Its group-cancel
question is answered by the parent-only authority boundary: worker callbacks cannot receive or
transfer the TaskGroup, and per-job mutexes linearize each cancellation request independently. The
public destructor's explicit-call semantics are stated below. Each descriptor registration owns its
own duplicate, so repeated registration has independent lifetime. iOS Parallel compile behavior is
pinned beside Async. Root failure remains a separate field, not an element of the ordered child list.
No other thread can race the synchronous boundary: any reentrant observation before `closed = true`
uses the live-scope terminal rule, and any observation during release after that first teardown write
gets the scope-ended Error. Queue starvation has no v1 guarantee. The
generic TaskGroupFailure message and copied monitor/trace-context timing are made explicit below.
The PHP serializer/unserializer is the current value payload codec inside the versioned EPV1 envelope;
other codec lowering remains future work.

## Forty-fourth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 44 states that requested cancellation never preempts the currently Running Async Fiber
and a queued started Fiber resumes only when selected at its FIFO turn; defines task-pending as every
non-terminal outcome; names the stable Parallel group-closed Error and TaskGroupFailure message;
spells out the atomic queued/running admission race, lack of queue backpressure/starvation guarantees,
group cancellation's parent-only authority, TaskGroup destructor behavior, root/child failure
separation, repeated stream-registration ownership, descriptor-close behavior, iOS Parallel AOT
refusal, and the current EPV1/PHP-value-codec relationship. The 54/58 three-host matrix remains the
current end-to-end scheduler evidence; supplemental macOS C-host injections remain separately open
for target-specific validation. Runtime code and tests remain unchanged from Candidate 35. All three
independent reviewers must review the complete specification at one identical frozen hash and return
complete `ACCEPT` verdicts with no findings or questions. Any material edit restarts the round. Kimi
K3 reviewed hash `89d89ec157af3a02419eed6895edcda210e79eb2d5cde47fed2dd78d115e6090` and rejected it.
Its two Async result findings are resolved by specifying standard PHP result aliasing and explicitly
allowing repeat awaits by the same task. Every `34/34` count in the 2026-09-21 ledger is now labeled
historical/superseded by Candidate 35's complete `58/58` matrix. Memory exhaustion has no v1
recoverable queue-full contract; allocator failure may be fatal/abort/OS termination and is not a
promised task error. Each Future's terminal state has one owner, with object aliases sharing that one
handle and `clone` prohibited; a group's explicit destructor behavior and post-close Error are named.
Repeated cancellation is an idempotent no-op that cannot replace the first reason. The Kimi questions
are answered in the same contract sections.

## Forty-fifth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 45 specifies PHP alias/COW semantics for repeated Async results, makes re-awaiting a
terminal result by the same task explicitly repeatable, labels the 34/34 ledger entries as historical
and superseded, states the non-recoverable host-failure semantics for unbounded queue memory
exhaustion, defines single-owner Future state with clone rejection, and pins repeated cancellation as
idempotent while retaining the first request. The current prelude source does not yet enforce the new
`Future`/Parallel `TaskGroup` clone prohibition; that source/AST/test parity fix remains an explicit
implementation gate after design consensus, not a claim of current runtime conformance. Candidate 40's
54/58 matrix remains the latest complete evidence for the implemented Candidate-35 runtime baseline,
not validation of Candidate-45-only semantics; the macOS-only fault-injection follow-ups are also
still explicitly open. All three independent reviewers must review this complete specification at
one identical frozen hash and return complete `ACCEPT` verdicts with no findings or questions. Any
material edit restarts the round. Kimi K3 reviewed hash
`e6bd4627fe8c177059cd650fb22d93e375e9f6ef991555963d0980c8cc648863` and rejected it. The started-
Runnable cancellation marker is now defined as orthogonal request-pending metadata, preserving queue
position until FIFO selection; `isComplete()` returns false for every live non-terminal target. A
cancelled queued job remains a private deque placeholder until an admission scan skips it. A failed
`join()` marks failure observed before throwing, and post-scope `join()` cannot mutate the already-
thrown group snapshot. Parent cancellation state records its first reason before per-job requests, so
the parent token observes it immediately after `cancel()` returns. The scheduler retains PHP resource
ownership through registration release, even if the caller drops its reference. Candidate 40's matrix
is clarified as Candidate-35 runtime baseline evidence only; Candidate-45-only contract additions
remain pending implementation/tests. Kimi's other questions are resolved: admission is FIFO without
starvation guarantees, the group failure message is stable/generic, Parallel trace/span propagation
is deferred, and the PHP serializer subset converts to/from EPV1 for native cross-context storage.

## Forty-sixth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 46 resolves the Candidate-45 lifecycle and observability questions by clarifying the
orthogonal queued-cancellation request marker, live `isComplete() == false` behavior, skipped deque
placeholders, join-time and post-scope failure-observation semantics, parent token reason visibility,
and stream-resource ownership through descriptor deregistration. It scope-limits Candidate-40's
54/58 matrix to the implemented Candidate-35 runtime baseline; later rules including Future/TaskGroup
clone rejection remain implementation and regression-test gates after design consensus. The trace
context paragraph and PHP-wire/EPV1 payload distinction state current versus deferred support. All
three independent reviewers must review the complete specification at one identical frozen hash and
return complete `ACCEPT` verdicts with no findings or questions. Any material edit restarts the round.
Kimi K3 reviewed hash `9b492199b2e335d4d6ab903ba37a4fcdb3f7724ff5a2ebb4226abf580be77999` and rejected it.
Its repeated lifecycle findings showed that the diagram was conflating task phases with request
metadata, Fiber continuation locations, and user exception handling. Candidate 47 now models only
actual phases and dispatch/wake transitions; the prose defines pending cancellation as orthogonal
metadata, catch/throw behavior, saved-point execution, wait-registration removal, root exemption,
and the no-preemption rule. The status/hash and exact candidate matrix boundaries remain explicit.

## Forty-seventh exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 47 replaced the over-detailed state diagram with task phases, normal wake/terminal edges,
and cancellation-related edges, while defining the request bit itself as orthogonal metadata and
exception delivery as ordinary PHP control flow.
It states `isComplete()`'s false result for every live non-terminal state; canceled queue placeholders
are skipped during admission; cancellation/failure observation linearization and post-scope snapshots
are fixed; and resource zval/descriptor lifetimes are explicit. Candidate-40 54/58 results remain
baseline evidence only for Candidate-35's implemented runtime, not the later clone/result contracts.
All three independent reviewers must review this complete specification at one identical frozen hash
and return complete `ACCEPT` verdicts with no findings or questions. Any material edit restarts the
round. Kimi K3 reviewed hash `e364741ab85b76092a2895eb3024072ca9dff86d30ad345c3b07912fae9e29dc` and
rejected it. Candidate 48 makes the never-started and started Runnable subcases explicit, shows normal
wakeups and non-root cancellation wakes from parked states, and accurately describes the diagram as
task phases plus event edges while the cancellation request remains orthogonal metadata.

## Forty-eighth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 48 clarifies the Async lifecycle diagram: never-started Runnable tasks and started Fibers
resuming from saved continuations have distinct dispatch edges; normal event wakes and cancellation
wakes both remove registrations before Runnable; a pending request is checked when a started task is
next selected, without preemption or an extra lifecycle phase. The text and precondition now describe
the diagram consistently. Candidate-40's 54/58 runtime matrix remains Candidate-35 baseline evidence
only; Candidate-45+ design requirements and clone guards are not represented as implemented/tested.
All three independent reviewers must review the full specification at one identical frozen hash and
return complete `ACCEPT` verdicts with no findings or questions. Any material edit restarts the round.
Kimi K3 reviewed hash `70830a1eb3f3b35bb46aaace31b5ce074290cb5dc813fa4a347ef9c6c4673050` and rejected
it. Its root-liveness question is resolved: cancellation never wakes a root parked on an unbounded
descriptor wait; with an active registration the executor may remain in `poll()` indefinitely rather
than raising the no-wake-source deadlock, while ordinary readiness/deadline lets the root continue
without injected cancellation. Reentrant Awaitable observations before the exact closed-bit write
see a live scope/terminal result; after it, both query and await return the scope-ended Error. The
Q16 shared-helper conflict is resolved by stating that checker validation is per worker admission
path; rejection on a Parallel worker path does not poison a parent-root/nested-Async use of the same
helper.

## Forty-ninth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 49 pins the indefinite-root-wait consequence of the cancellation exemption, names the
exact pre/post `closed = true` outcomes for reentrant `isComplete()` and `await()`, and makes Q16's
global/static analysis explicitly per worker-task admission path even where helpers are shared with
same-context Async tasks. Candidate-48 diagram clarifications remain. Candidate-40's matrix is still
Candidate-35 baseline evidence only; later design rules remain pending implementation and tests. All
three independent reviewers must review the full specification at one identical frozen hash and
return complete `ACCEPT` verdicts with no findings or questions. Any material edit restarts the round.
Kimi K3 reviewed hash `44b69efcbcadcc4cb1edb891765c6f3f97dba3391b8eeeca9e5ce69ed9164352` and rejected
it. Its dispatch-order question is answered by Candidate 50: FIFO dispatch marks a task Running
before resuming its saved continuation, then the explicit check observes a pending cancellation
before the next ordinary PHP statement. `isComplete()` and `await()` share the explicit live/closed
matrix above. The blocking join contract states that new sibling Async requests cannot arise while
the scheduler is blocked, and any pre-existing request is deferred to the later explicit point.
Parent-token reason state is independent of worker terminal publication timing. The `failures()` list
is the submission-ordered subsequence remaining after observation-based exclusions. The stream zval
is retained through deregistration. iOS AOT diagnostics and both failure-path messages are explicit.
The 28/88 Linux run is marked pre-fix, the same test selection's 88/88 result is post-fix, and no tests
were removed. Early 3/4-block heap measurements are marked pre-fix with the subsequent cleanup fixes
identified. Q16 now states per-admission classification and why a worker rejection does not poison a
separate parent/nested-Async call path.

## Fiftieth exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 50 pins the exact phase boundary for FIFO dispatch versus continuation cancellation checks,
reaffirms the live/closed Awaitable observation matrix, bounds cancellation guarantees around a blocked
`Future::join()`, and separates parent cancellation-token reason from per-worker job terminalization.
It states that observed failure removal preserves the relative order of remaining child failures,
that the scheduler retains the stream zval until descriptor release, and supplies exact compile-time
iOS diagnostics. The historical Linux 28/88 failure and later 88/88 same-selection repair, and the
pre-fix heap leak counts followed by cleanup fixes, are labeled as sequential historical evidence.
Shared helpers are classified per Parallel worker admission versus parent/Async call path. Candidate-40
runtime matrices remain baseline-only; later contract changes remain explicit implementation/test
gates. All three independent reviewers must review the complete specification at one identical frozen
hash and return complete `ACCEPT` verdicts with no findings or questions. Any material edit restarts
the round. Kimi K3 reviewed hash
`5577da57a4e5b2f3c7655a069b4601d0229b95538138e79c06057395543f2649` and rejected it. Its diagram
claims F-001/F-002 are not accepted as missing transitions: the full diagram already contains both
the normal wake edge and the non-root cancellation-wake edge. Candidate 51 clarifies the relevant
prose. A root's explicit `Cancellation::throwIfRequested()` throws on a requested token even though
scheduler-injected cancellation is exempt at root wait points. Successful `spawn()` returns after
enqueue, never waits for admission; `join()` on Queued waits in place and cannot reorder admission.
Resource-zval retention is internal ownership: dropping the caller reference does not destroy the
resource until deregistration; no exact refcount is a public contract. The iOS diagnostics are exact
compile-time messages. The 28/88 failure was fixed in lowering with the same 88-test selection, not
by test removal. The early block-retention figures were parent-side, pre-fix diagnostics and were
closed by the later activation/caller-cleanup fixes. Q16's per-admission classification rules out a
global/static bypass: the submitted worker callable is separately rejected before job creation.

## Fifty-first exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 51 states that the root exemption covers only scheduler-injected wait-point cancellation,
not an explicit token `throwIfRequested()` call; `spawn()` never waits for worker admission and a
queued `join()` does not change FIFO priority. It defines the stream zval's extended lifetime as an
internal ownership side effect without promising a refcount value, and records the exact iOS
compile-time diagnostics. The Linux 28/88 result is explicitly the pre-fix run of the same 88-test
selection, with no tests removed; the early heap counts were parent-side pre-fix evidence later closed
by fixes. Worker global/static safety remains checked per submitted worker call graph, so an allowed
parent/Async helper does not bypass the worker-path gate. Candidate-40 matrices remain baseline-only.
All three independent reviewers must review the full specification at one identical frozen hash and
return complete `ACCEPT` verdicts with no findings or questions. Any material edit restarts the round.
Kimi K3 and GLM 5.2 Cloud both returned complete `ACCEPT` verdicts with no findings/questions on
hash `5b81ef47e418b64722f45a0bcfb1859916ca414d9fb89ab8705b697730a5b0f5`; GLM 5.3 Cloud did not
produce a complete final verdict and is not counted. DeepSeek V4.1 Flash reviewed that same hash and
rejected it. Candidate 52 specifies that an outside-task await of a live terminal Cancelled child
throws a fresh request-paired exception carrying the first reason; a blocked join on Queued wakes
when cancellation changes its phase to terminal Cancelled; and a root awaiting a cancelled child is
woken by that child's terminal completion, not by the root-excluded group-cancellation wake. It also
defines `Future::isComplete()` by the mutex-protected phase (Queued cancellation transitions directly
to terminal; a Running request alone is not completion) and explicitly states the closed Awaitable
guard overrides terminal `isComplete()` results.

## Fifty-second exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 52 resolves DeepSeek's five Candidate-51 findings with the outside-task Cancelled-await
outcome/reason, queue-cancel wake for a blocked Future join, root await-on-cancelled-child completion
wake, phase-based Future completion observation, and strict live-versus-closed Awaitable outcome
matrix. Candidate-40 matrices remain Candidate-35 runtime baseline only; clone guards and later rules
still require implementation/tests after design consensus. Kimi K3 reviewed hash
`00429ec8bb8fe8d4e68fc0622ab81b9d76bcd82e3fe4a2358924734e5a605aec` and rejected it. Its `WaitingTask`
diagram finding is fixed by showing child terminal completion (including Cancelled) as the ordinary
wake edge; a Cancelled await itself is not a Failed-outcome observation. A Queued cancellation
publishes terminal `Cancelled` and signals the per-job condition variable before waking blocked
joiners. The job mutex atomically arbitrates `mark_running()` against cancellation, and executor
dequeue does not remove the job from the parent TaskGroup's cancellation registry. Kimi's remaining
finding exposed an actual root-entry bypass: the then-current Async scheduler/scope/TaskGroup/Awaitable
constructors and `__Scheduler::runRoot()` were callable implementation entry points. Candidate 53
initially proposed private constructors and `runRoot()` with narrow compiler-owned friend access;
Candidate 54 narrows the authority boundary to private `runRoot()` after auditing the GC and slot-
generation probes that construct inert internal records.

## Fifty-third exact-hash review precondition — 2026-09-25 (historical; rejected below)

Candidate 53 explicitly separates parent-owned execution registration from per-job cancellation, adds
terminal notifications for blocked joins and exact root/Cancelled await behavior, and closes the
Async alternate-root-construction path: only `Async\\run()` may access private scheduler/scope/root
constructors and `runRoot()`, while `TaskGroup::spawn()` alone creates `Awaitable`s through its friend
entry. Reflection bypass is explicitly outside the integrity guarantee. The current source does not
yet enforce these new private-constructor/friend rules; that source/AST/diagnostic regression remains
open after design consensus alongside Future/TaskGroup clone rejection. Candidate-40 runtime matrices
remain Candidate-35 baseline only. Kimi K3 reviewed hash
`9ad60d844287d29e1a300d10abcaf72c3d6b80a1a89f933fb2230acd2ce78321` and rejected it. F-001
overlooked the explicit root wait-for-descendants rule; Candidate 54 repeats its return boundary
beside the public API. F-002/F-003 requested `reschedule()` as a named non-root cancellation point
and `isComplete()` as explicitly callable outside a task while live; both are now stated. F-004's
checklist concern is answered by labeling early review reconciliation historical only. F-005
requested worker-side fail-closed group authority; the private prelude guard now checks worker
context before touching group state and its reflection-forged worker reproducer passes on macOS.
No other model reviewed Candidate 53.

## Fifty-fourth exact-hash review precondition — 2026-09-27 (historical; superseded before review)

Candidate 54 makes `__Scheduler::runRoot()` private with compiler-owned access from `Async\\run()`.
The four internal constructors retain their exact public implementation signatures: they can build
inert records for GC probes but cannot drive a root or attach to an active scope. A direct user call
to `runRoot()` raises the private-method `Error`; the internal slot-generation regression invokes it
through Reflection, which remains outside the supported scope-entry guarantee. Parallel `Future`
and `TaskGroup` have private `__clone()` hooks in both PHP source and generated AST, and group methods
reject a valid worker-local forged receiver before accessing scope state. macOS focused evidence
passes for source/AST parity in both preludes, private clone diagnostics (2/2), public Async root,
private `runRoot` refusal, stale-handle generation, forged-group isolation, three Async heap-debug
reducers, and the worker-side group-guard reproducer. `rustfmt --check` and `git diff --check` pass.
Linux focused reruns and the current complete code review remain open; Candidate-40's 54/58 matrix
describes the earlier runtime baseline. Kimi, GLM, and DeepSeek must review this complete specification
at one identical frozen hash and return complete `ACCEPT` verdicts with no findings/questions. Any
material edit restarts all three. The first Kimi cloud call on hash
`b67e42f52a6ef256ee0b8e9e6bc4ff0fd25ef0f657bad13fe6993b2e3a525345` returned Ollama's weekly usage-
limit message, not a reviewer verdict. No Candidate-54 model review counts toward consensus.

## Fifty-fifth exact-hash review precondition — 2026-09-27 (historical; superseded before review)

Candidate 55 retains Candidate-54's API and runtime changes. The five focused new/affected filters
pass on macOS AArch64, Linux x86_64, and Linux AArch64: worker-local forged TaskGroup rejection,
direct private `runRoot()` refusal, stale-generation reuse through test-only Reflection, and both
Parallel clone diagnostics. The source/AST parity oracles for Async and Parallel pass on macOS;
three Async heap-debug reducers pass through the public root boundary. The current macOS codegen
slices also pass completely: `codegen::async_scheduler` **55/55** and `parallel_` **59/59**. `cargo build`, focused
`rustfmt --check`, and `git diff --check` pass locally. The historical Candidate-40 54/58 matrix does
not substitute for full post-change Linux suites, and iOS target/refusal evidence has not been rerun
after this delta. Ollama cloud model reviews and the dedicated complete code review remain pending until
the weekly quota resets; no reviewer lock exists for this hash. Kimi, GLM, and DeepSeek must each
return a complete exact-hash `ACCEPT` with no findings/questions before any push.

After this evidence snapshot, hardening cleanup for a constructor-bypassed TaskGroup placed an
`isset($this->state)` early return before the already-closed branch. Because normal `__close()` unsets
`state`, that order suppressed the later parent-guard release. The macOS `parallel_` slice reproduced
the regression as **54/59**: five parent-Fiber, retained-group, and heap-cleanup cases failed with a
spurious nested-scope error or wrong guard precedence. No model reviewed the Candidate-55 hash. The
guard was moved behind the closed fast path and is validated in Candidate 56 below.

## Fifty-sixth exact-hash review precondition — superseded before review, 2026-09-27

Candidate 56 keeps a public Future/TaskGroup clone refusal, private Async `runRoot()` entry, and the
worker-context TaskGroup guard. The destructor now releases a valid closed group's parent guard
before checking whether an open forged object has uninitialized `state`. The current macOS slices
pass Async **55/55** and Parallel **59/59**, including all five cases that failed at the intermediate
ordering. The forged-worker and retained-group root-exception reducers pass **2/2** on both Linux
x86_64 and Linux AArch64; the other five focused new/affected paths also passed on both Linux hosts
before this cleanup-only reorder. PHP-source/AST parity passes for Async and Parallel. Linux full
post-change suites, cloud model reviews, and the dedicated complete code review remain open. Focused
iOS device and Simulator tests pass **1/1** for Async library refusal plus `--check`, and **1/1** for
Parallel library refusal plus `--check`; neither target claims runtime execution. Kimi, GLM, and
DeepSeek must each return a complete `ACCEPT` on one frozen hash with no
findings/questions before publication. `cargo build`, focused `rustfmt --check`, and `git diff --check`
pass on the corrected tree.

The `update-builtin-docs` workflow then found that all five public Parallel prelude types were absent
from the neutral class catalog. It added `Future`, `TaskFailure`, `TaskFailureKind`, `TaskGroup`, and
`TaskGroupFailure` as Elephc extension prelude types, amended the generated symbol registry and
documentation, and added a two-way AST/catalog regression. No model reviewed Candidate 56's hash.

## Fifty-seventh exact-hash review precondition — superseded before review, 2026-09-27

Candidate 57 includes the five Parallel class/enum contracts and the regenerated builtin docs. The
registry delta from HEAD is exactly five Async functions plus fifteen Async/Parallel classes, with no
removed builtin or class; constants remain 1097. The `update-builtin-docs` build/render workflow
produced 1078 builtin records and 209 classes; its builtin audit found zero errors, site validation
accepted 2075 generated pages, and the typed EIR boundary audit found zero structural errors. The
Async and Parallel prelude AST class declarations each agree exactly with the neutral class catalog;
both two-way unit gates pass. Current macOS codegen slices pass Async
**55/55** and `parallel_` **62/62**. On each Linux executable host, the three new catalog/strict-PHP
probes and five affected scope/clone reducers pass individually; the final destructor cleanup cases
also passed on both Linux hosts after the closed-before-`isset` ordering repair. iOS device and
Simulator refusal/`--check` tests pass for Async and Parallel on this tree. Bridge and contract unit
slices pass **41/41** and **4/4**; Async and Parallel prelude slices pass **5/5** and **6/6**, and the
`parallel_` diagnostic slice passes **37/37**. `cargo build`, focused
`rustfmt --check`, and `git diff --check` pass. Candidate 57 received no model verdict. Its open
gates were carried into Candidate 58 below; no prior verdict applies to Candidate 58.

## Fifty-eighth exact-hash review precondition — historical; superseded by Candidate 61

The publication-scope audit found a user-facing `--with-parallel` ambiguity: unlike ordinary
prelude-backed bridge flags, this flag links only worker infrastructure and leaves the Parallel
PHP declarations absent when no `run` reference is detected. The existing link-only CLI fixture
contains no `run` reference, asserts that `class_exists('Elephc\\Parallel\\Future')` is false, and
passes on macOS AArch64. The detector is deliberately described as conservative: its
pre-resolution final-segment matching can inject for unrelated `run` references. The CLI reference,
linking guide, and Parallel user guide state the force-link exception and detection boundary. This is
an API clarification, not a runtime behavior change. Candidate 57 received no model verdict; all
three reviewers must inspect Candidate 58's exact frozen hash. The complete code review,
publication-scope audit, full post-change target matrix, and push remain open.

## Candidate 59 exact-hash review outcome — rejected

Candidate 59 specification hash 77cfcacab7a99b73d8728178ac66254d058f0dc7a7bb2e975466fc7e3d2b82c0
was reviewed by Kimi K3, GLM 5.3, and DeepSeek V4.1; all three returned BLOCK. No Candidate 59
verdict transfers to another specification or code tree.

The blocking reconciliation is:

- Candidate 58 remained labeled as the pending precondition while Candidate 59 was appended after
  the consensus procedure and sources, so the authoritative review target was ambiguous.
- The seven-task-argument cap narrowed the previously accepted variadic Async API merely to fit the
  current Fiber start ABI. It is rejected as the v1 public contract.
- Candidate 59 did not pin the exact diagnostics and order for static versus dynamic overflow or
  their precedence against a closed scope and cancellation.
- Normative references must use Awaitable::await(), not the nonexistent TaskGroup::await().
- The specification must state Parallel's independent variadic argument policy and distinguish a
  direct raw Fiber::suspend() from a dynamic callable that resolves to that method.

## Sixtieth exact-hash review precondition — historical; superseded before review by Candidate 61

Candidate 60 preserves the public Async TaskGroup::spawn(callable $task, mixed ...$args) shape
without an arbitrary seven-argument ceiling. Counts through seven retain the existing direct Fiber
launch. Above seven, the child scheduler stores the callable and variadic argument array in an
internal static closure that invokes the original callable with argument unpacking when the child
Fiber starts; that Fiber itself starts with zero arguments. This is a transport choice, not
admission validation: every count is accepted subject to normal PHP call semantics and resources.
This is the proposed implementation boundary; it is not yet implemented or accepted by reviewers.

For more than seven task arguments, the closure trampoline adds an observable internal closure
frame to debug_backtrace() inside a task (the direct callback and trampoline probes return
different frame counts). That behavior must be disclosed in the implementation review. It is not a
promise of direct Fiber stack-trace parity.
If any reviewer finds that trade-off unacceptable, preserve the unbounded API and expand the work
to a lower-level Fiber/descriptor argument transport; do not reinstate the seven-argument cap.

Admission order remains explicit: TaskGroup scope access reports the ended-scope Error first;
then the scheduler checks cancellation and rejects a cancelled group's new spawn before child
creation/enqueue. The task body is invoked only when its scheduled Fiber runs. There is no separate
Async arity-overflow diagnostic: statically known arguments and dynamic spreads are both accepted
subject only to normal call semantics and process resource limits. Exceptions raised by the task
body continue through the existing child outcome and Awaitable observation path.

Raw Fiber::suspend() is not an Async scheduler wakeup. Calling it directly inside an active Async
task raises Error("Fiber::suspend() is not a scheduler wakeup inside an Elephc Async task").
A dynamic callable that resolves to Fiber::suspend must hit the same guard before dispatch; it
must not leave the scheduler with a non-runnable child and surface only as a deadlock. Outside an
active Async task, ordinary Fiber suspension remains a Fiber operation, not a TaskGroup yield.
Normative wait APIs are Awaitable::await(), TaskGroup::reschedule(), sleep(), and descriptor
waits; TaskGroup::await() is not an API.

Parallel remains independent of the Fiber start ABI. Its own variadic TaskGroup::spawn() has no
seven-argument Fiber-derived cap; each argument remains subject to the reviewed copied-transfer
and serialization contract. Candidate 60 requires an end-to-end Parallel regression with more than
seven task arguments.

Before freezing the next code tree, the focused evidence must include static and dynamic Async
spawn calls above seven arguments, source/generated-AST parity, task failure and cancellation
regressions, Parallel arity, and the supported-target paths affected by the change. The current
Linux x86_64 QEMU run is still testing the pre-Candidate-60 worktree; it is not evidence for the
new Async implementation. The Candidate 60 code tree, specification hash, dedicated Codex review,
Kimi/GLM/DeepSeek reviews, and publication audit all remain open.

## Sixty-first exact-hash review precondition — historical; rejected, 2026-09-30

This was the sole frozen Candidate 61 target at review launch. Its immutable specification and
code hashes are retained in the review log and detached checkout. The blanket Hash collector
implementation described below was rejected after downstream consumer/return/binding regressions
were reproduced; it is not the current storage requirement.

Candidate 61 implements unbounded Async spawn arguments with one static zero-argument runner for
every child, including calls with zero through seven arguments. The runner calls
`call_user_func_array($body, $args)` only when the child Fiber runs. This preserves deferred
execution and both numeric and named argument keys without relying on raw Fiber start argument
capacity. An additional internal closure frame is observable at every arity. Admission order is
closed-scope access first, then cancellation, then child registration; there is no Async-specific
argument-count overflow diagnostic. Ordinary task argument failures follow child failure/await
semantics. Parallel remains independent and accepts more than seven copied-transfer arguments.

Named variadic captures exposed a shared EIR ABI defect outside Async too: a capture can be a Hash,
while an indexed-Array callee slot assumes a different layout. User-function, method, and closure
variadic EIR storage now uses `AssocArray<Mixed, Mixed>`. The original PHP signature remains the
argument-planning and coercion authority; positional containers convert to numeric-key hashes, and
typed associative containers convert to boxed Mixed storage. Runtime-selected callbacks retain
their declared variadic element binding, and inferred variadic-container returns admit either
runtime array shape. Review must inspect all shared call surfaces and ownership paths affected by
this change, not only Async tests. Runtime invoker temporary stack slots remain ABI-aligned on
AArch64 and x86_64.

Focused macOS evidence after the final functional changes: `async_spawn` 7/7,
`state_and_variadics` 53/53, `dynamic_assoc_args_for_variadic_callback` 4/4,
`call_user_func_array_dynamic_string` 8/8, Parallel spawn above seven arguments 1/1,
Async source/generated-AST parity 1/1, and the two-architecture invoker stack-slot unit test 1/1.
The Async and general variadic groups were rerun after removing the temporary diagnostic probe.
`cargo fmt --all -- --check` and `git diff --check` passed. These results are focused host evidence;
they do not establish complete supported-target or review acceptance.

The last Linux x86_64 QEMU run exited without a captured runtime verdict. A host cross-check could
not compile `ring` because `x86_64-linux-gnu-gcc` is absent. Neither attempt is a code pass or fail.
Current-tree Linux AArch64/x86_64 runtime checks, iOS compile/refusal checks, generated-doc and
assembly audits, independent dedicated Codex and Kimi/GLM/DeepSeek reviews, publication-path audit,
and final push gates remain open. Prior verdicts never transfer to this candidate.

The PHP IO-hooks gist is a related research reference only. It does not expand the accepted v1
scope or authorize transparent blocking-I/O interception, a new public provider API, or a Ring
backend as part of this candidate.

## Sixty-second implementation and review precondition — current draft, 2026-09-30

This section is the sole current precondition. Candidate 62's complete source tree is not frozen
and has no accepted model verdict. The companion identifies the current specification bytes;
Candidate 61's archived checksum identifies its historical rejected artifact separately.

The public structured Async/Parallel boundaries, unbounded spawn arguments, deferred zero-argument
child runner, scope authority, cancellation/failure ordering and explicit target refusals remain as
specified above. The runner frame remains observable at every arity. There is no arity cap or new
public I/O/scheduler-provider API.

The storage correction must preserve the PHP-array contract independently of physical indexed or
associative representation. Known positional collectors retain their indexed element layout and
fast paths; unknown-shape boundaries use an array-only raw-pointer carrier that admits both Array
and Hash. Declared element binding, reference-cell semantics, reflection metadata, result typing,
key preservation/renumbering, COW and cleanup remain separate authoritative properties. Declared
PHP `array` arguments/returns may not treat a Hash pointer as an indexed Array or discard named
keys. Every previously supported positional array consumer must remain supported. The complete
design, source-grounded consumer inventory and counterexamples are in
[`execution-scheduler-php-array-abi.md`](execution-scheduler-php-array-abi.md); that work is required
for acceptance, not an optional future cleanup.

Live fixes additionally cover cloned Async group authority, fail-fast cancellation after a caught
Parallel joined failure, retention of failed/cancelled settled outcomes, detached Future ownership,
and scoped namespace/function alias detection with literal callable-string lookup. Dynamic declared
callback parameters must reject invalid values catchably before marshalling owners, preserve valid
weak numeric-string and Stringable conversion, and release preceding owners/collectors if coercion
throws. The weak `call_user_func_array`/scheduler path must not be confused with direct caller
`strict_types` policy; any unresolved direct-descriptor policy remains an explicit review issue.

Acceptance requires the array ABI inventory and regression matrix, source/generated-AST parity,
scope/failure/heap-clean regressions, descriptor binding/return/ownership checks, current-source
Linux AArch64/x86_64 runtime evidence and iOS target-path checks, generated builtin and assembly
audits, a curated publication-path manifest, and complete independent dedicated Codex plus Kimi,
GLM and DeepSeek acceptance for one exact specification/code artifact. Partial source inspection,
provider quota/tool failures, or old green tests cannot substitute for any gate. Kimi/GLM's Candidate
61 attempts ended at Ollama quota without verdicts; DeepSeek's attempts ended on tool protocol
errors. Correct source first, then freeze Candidate 62 and restart the complete reviewer round.

## Consensus procedure

1. The maintainer reviews the proposed answers and resolves or explicitly defers every question.
2. Update this file so accepted statements are marked **LOCKED**, rejected alternatives are recorded
   with their rationale, and no conflicting older API remains in `sandbox-threads.md`.
3. Compute the exact SHA-256 of this file and publish it in the companion file
   [`execution-scheduler.sha256`](execution-scheduler.sha256). That companion is the sole hash
   authority: do not embed the digest into this file, which would change the bytes being hashed.
4. Independent reviewers receive that exact hash and a read-only task. A review of another hash,
   an unlocked document, or an incomplete transcript does not count.
5. Reproduce each blocking finding against the source or a focused design counterexample.
6. Amend the specification, generate a new hash, and repeat review when a material decision changes.
7. Persist the final decisions with Kioku. The maintainer already opened the implementation gate
   for this locked v1 scope; any future widening beyond that scope requires explicit approval.

## Sources consulted for this draft

- Elephc `sandbox-threads.md`, runtime context implementation, Fiber runtime and docs, PCNTL safe
  points, web worker loop, request-state bridge, and monitoring contract in this worktree.
- Linux kernel [EEVDF](https://kernel.org/doc/html/latest/scheduler/sched-eevdf.html) and
  [CFS](https://kernel.org/doc/html/latest/scheduler/sched-design-CFS.html) scheduler documentation.
- PHP [Fiber manual](https://www.php.net/fibers) and
  [accepted Fiber RFC](https://wiki.php.net/rfc/fibers).
- Java 25 [`StructuredTaskScope`](https://docs.oracle.com/en/java/javase/25/docs/api/java.base/java/util/concurrent/StructuredTaskScope.html)
  documentation.
- [AMPHP](https://amphp.org/amp) documentation for Fiber scheduling, Futures, cancellation, timers,
  and blocking-I/O constraints.

## Candidate 59 — rejected Async spawn argument bound (historical)

The public Async spawn surface remains variadic, but the task argument list is bounded to seven in
v1 because `Fiber::start()` and the current start-argument storage ABI support at most seven values.
Known overflow is diagnosed during type checking. A dynamic spread is checked at `TaskGroup::spawn()`
before it creates or enqueues the child. The internal `__TaskStart::start()` check remains a
defensive invariant, not the public error path.
