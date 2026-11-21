# Scheduler heartbeat discussion

## Checklist

- [x] Discuss developer use cases: periodic application work with independent intervals.
- [x] Distinguish scheduler observability, worker liveness and application periodic work.
- [x] Select TaskGroup::every in Elephc\Task; maintainer authorizes implementation.
- [ ] Specify monotonic timing, scope ownership, cancellation and teardown if retained.
- [ ] Validate dormant cost, event volume and behavior when PHP or native code blocks.

## Request and preliminary framing

Current authority: implementation is now authorized by the maintainer; see
`.plans/task-api-unification.md`. The sections below preserve the design discussion
history. Earlier "not authorized" statements describe their historical phase, not
the current implementation permission. Tests, documentation and review gates are open.

The maintainer requested a discussion on 2026-10-09 about a heartbeat approach
that could provide pleasant developer-facing scheduler use cases. This is a
discussion item, not authorization to introduce a public callback API or change
the locked scheduler specification during main integration.

Keep three concepts separate:

1. An observation heartbeat reports scheduler progress or lack of progress. It
   could support diagnostics, metrics, progress/status displays and identifying
   scopes that remain alive without making useful progress.
2. A worker-liveness heartbeat helps a supervisor distinguish a responsive worker
   from one that may be blocked. An OS thread remaining alive is not evidence that
   its PHP task is progressing, and heartbeat loss cannot safely kill that thread.
3. Periodic application work is an actual scoped task, with explicit failure,
   cancellation, backpressure and teardown semantics. It must not become an
   implicit detached-task factory disguised as instrumentation.

For cooperative Async, a callback executed on the same event loop cannot report
while that loop is blocked. Silence may be the signal; a watchdog requires an
independent observer. Parallel's thread isolation adds a separate ownership and
measurement boundary. None of these implies preemption or hard real-time timing.

Potential use cases to examine, not promised features: status/progress reporting,
stall diagnostics, health reporting, developer debugging, and scoped periodic
maintenance. Avoid conflating one healthy scheduler tick with useful progress of
every task. Preserve the distinct Async Awaitable and Parallel Future contracts,
run() root ownership and TaskGroup::spawn() task ownership.

Questions for consensus: who observes it, whether developers supply code or only
consume events, the monotonic interval and tolerated jitter, whether slow callbacks
coalesce or queue, how throws/cancellation are handled, and whether existing
monitoring events already cover the useful signal. No PHP/Rust API is selected yet.

## 2026-10-11 observation proposal after the main rebase — NOT LOCKED

The maintainer asked to revisit heartbeat while finishing the scheduler. The
recommended first slice is **observability, not a developer callback**: a bounded
scheduler snapshot/event for monitor/debug consumers, owned by the existing run
scope and emitted only while a capture/observer is active. No implementation or
public signature is approved by this proposal.

The maintainer subsequently selected **periodic application actions** as the
intended heartbeat use case. This supersedes the observation-first recommendation
as the priority for this discussion, not the distinction between observation and
application work. The observation design below remains an optional follow-up;
no heartbeat API or implementation has been approved.

Source grounding: `__Scheduler::drive` already wakes timers/readiness, selects FIFO
tasks and waits on poll or monotonic deadlines. `monitorTask` publishes state/reason
transitions through `__elephc_async_monitor_event`. The PHP source is a parse/parity
oracle for compiler-owned AST construction, not authorization to edit only that
oracle. Any eventual change must update the AST builder, shared monitoring contract,
both backend/runtime conventions and tests together.

Keep three signals separate:

| Signal | Possible observation | What it must not claim |
|---|---|---|
| Scheduler pulse | The loop reached an observation point recently | Every task progressed |
| Progress counters | Dispatches, completions, failures or wakeups changed since a snapshot | A task will complete or meet a deadline |
| Independent watchdog | Time since last observed loop/worker acknowledgement | Permission to kill a thread, skip cleanup or release its heap |

A candidate snapshot could carry scope/context generation, monotonic observation
time, pulse sequence, dispatch/completion sequences, runnable/waiting counts and
known wait category. These are proposed fields, not a selected ABI. Scope/generation
is essential to reject late observations after teardown or arena reuse. It must
not retain PHP values, callback pointers or worker arenas.

Cadence is a maximum observation opportunity under cooperation, not a real-time
promise. A slow PHP/native call on the loop prevents an in-loop pulse; a progress
display must show stale/unknown rather than invent health. Sleeping or waiting for
I/O with no runnable work is not by itself a stall. Missed intervals coalesce into
one current snapshot, never an unbounded replay queue. Poll/usleep may need a
minimum observer deadline ONLY while an observer is active; that wakeup/energy cost
must be measured explicitly and must not count as a user task or prevent scope exit.

Dormant requirements: no background thread, task, per-tick allocation or additional
clock read when observation is disabled. Reuse a monotonic timestamp already needed
by a loop step when available. Observation must not synchronously execute PHP code,
re-enter task dispatch, mutate cancellation, or become a second event loop. It
stops before teardown and emits no events against a retired scope.

Parallel must distinguish scheduler/admission progress from worker progress. A
supervisor pulse while a worker is blocked must not report that worker healthy;
an isolated worker snapshot exchanges scalar/copy-safe data, never PHP pointers.
An independent watchdog can diagnose elapsed silence but cannot preempt or destroy
the running PHP task under the existing v1 ownership contract.

Periodic application work remains an ordinary explicitly spawned child task, with
failure/cancellation/backpressure and an explicit stop condition before the root
scope can finish. A helper such as `every()` would be a separate library/API
discussion; it must not be hidden behind the observation heartbeat.

Acceptance before implementation: decide inclusion in Candidate62 versus a follow-up;
define intended consumers and a measurable benefit over existing transition events;
then test fake monotonic time, jitter/coalescing, empty/waiting scopes, cancellation,
throw/teardown, recycled IDs, blocked PHP/native code, and dormant/active overhead on
all supported targets. No heartbeat feature is marked implemented by this document.

## 2026-10-11 maintainer clarification — periodic application work

Confirmed need: execute application actions periodically. API and detailed
semantics remain proposed, not locked. Treat this as an ordinary structured child
task using existing timer/suspension facilities, not a callback injected into each
scheduler tick, a detached task, a worker-liveness signal or a new scheduling policy.
Preserve `run()` root ownership and `TaskGroup::spawn()` as the v1 creation boundary.
The maintainer explicitly requested API discussion before heartbeat implementation;
do not implement a periodic surface or treat the defaults below as accepted.

Recommended default for discussion:

- Start after the first interval; immediate first execution must be an explicit choice.
- Run serially within one periodic task; never overlap its own invocations by default.
- Prefer a fixed monotonic cadence with missed deadlines coalesced: after an action
  completes, wait for the next future deadline, without replaying all missed ticks.
  Contrast this with fixed-delay timing (interval after completion) before selecting
  a contract; neither promises hard real-time execution.
- Use the existing task cancellation and exception contract. Cancellation must wake
  an interval wait; a thrown action must not be silently swallowed or retried forever.
- Require an explicit lifecycle/stop condition: an infinite periodic child prevents
  its owning scope from settling until cancelled, even after the root body returns.
- Keep cooperative limitations visible: blocking PHP/native code delays both this
  task and other work on the same loop; a timer cannot preempt it.

Before any public helper (for example an `every()` operation) is chosen, demonstrate
a small timer-loop composition through the compiler-owned AST path and verify how
it fits the locked task-creation contract. Prefer existing primitives if they cover
the use case. Do not prescribe a callback signature, result type, retry policy or
cross-worker transfer API without consensus.

Acceptance probes: slow actions and no overlap, skipped deadlines without a burst,
monotonic clock/jitter, cancellation while waiting and during cooperative work,
action failure, scope teardown and ownership cleanup. Cover every supported target
for any implementation, and measure idle wakeups only while a periodic task exists.
The current priority remains closing scheduler heap/ownership regressions before
adding this surface to a reviewed candidate.

## Historical API discussion — per-repetition control (collective direction supersedes)

Maintainer preferences confirmed in discussion (design only, no implementation):

- Prefer `every(float $seconds, callable $action)` to a separate Interval/wait API.
- Run `every()` inside an explicitly spawned task; pass scope/context through
  closure arguments rather than requiring `use` captures. Existing explicit
  `spawn($callback, ...$args)` transport does not imply automatic TaskContext injection.
- Prefer a visible control object passed to the periodic callback over a boolean
  return value that implicitly stops repetition.
- Explore `setInterval()` on that control object to change the current repetition's
  interval. It must not change the whole TaskGroup or unrelated tasks.

Draft surface, NOT LOCKED:

```php
$scope->every(1.0, function (PeriodicControl $control): void {
    refreshCache();
    $control->setInterval(5.0);
});
```

`PeriodicControl`, `stop()` and `setInterval(float $seconds)` are proposed names.
Recommended semantics still requiring agreement: stop is local/cooperative, the
current callback finishes, and interval updates made within the callback take
effect afterward. On an actual interval change, reset the next deadline to callback
completion plus the new interval; subsequent unchanged periods follow monotonic
fixed cadence with missed deadlines skipped. Multiple updates in a callback use
the last valid value; stop takes precedence. Reapplying the same interval should
not silently reset cadence. Reject nonpositive/nonfinite/overflowing durations.

Not yet decided: whether control may be retained and used by a different task
while this repetition waits; if allowed, define timer replacement/wakeup, access
ownership and post-stop/post-scope invalidation. Do not promise cross-worker control
or interruption of executing callbacks. Do not equate this local control with the
whole group, a task handle, cancellation authority or scheduler liveness.

## Historical collective-cadence proposal — superseded by hub clarification

The maintainer explicitly prefers a collective model: one shared cadence pilots
multiple participating actions/tasks. This supersedes the recommendation to limit
the model to isolated per-action repetitions. Keep the accepted preferences for
an explicit interval, a visible control object and explicit closure arguments.
No heartbeat implementation is authorized, no signature/timing policy is locked.

Distinguish the structured owner (`TaskGroup`) from a periodic cohort/heartbeat.
Participation must be explicit: unrelated children in a TaskGroup must not become
periodic or restart merely because the group has a heartbeat. The mechanism repeats
actions by fresh invocations; it does not restart completed task handles or rerun
arbitrary task bodies. Named task lookup/public IDs are not required if the cohort
is defined by callback composition and direct references.

Proposal to discuss: a central periodic callback receives heartbeat control and
the current TaskGroup explicitly, creates one batch with existing `spawn()`, and
awaits that batch before returning. This preserves spawn as the creation boundary
and supports cooperative concurrency within a beat without hidden detached work.
Suggested collective policy: no overlap between batches, missed beats skipped,
normal scope failure/cancellation propagation. These are recommendations, not
maintainer-approved semantics. A group-wide completion barrier or fresh per-beat
child scope is not an existing primitive promised by this document.

`setInterval()` would govern the cohort's next beats, not the scheduler itself or
unrelated children. `stop()` would prohibit further batches, not forcibly terminate
the current one. Define when requested changes become effective if several tasks
may control a beat; do not reuse the historical callback-completion reanchoring
rule as an accepted collective contract. Lifecycle/ownership, API names, callback
arguments, slow/failing actions, registration/unregistration and any externally
shared control remain open. Discuss the collective API before implementation.

## Current direction — heartbeat hub with per-registration intervals

The maintainer clarified the intended collective model: a hub triggers X periodic
actions/tasks, each with its own potentially different interval. Centralization is
of registrations, control and deadline management, NOT a single shared cadence or
a batch-wide completion barrier. This supersedes the previous common-cadence
interpretation. No implementation or exact public signature is approved.

Subsequent maintainer clarification: put `every()` directly on `TaskGroup`, beside
`spawn()`. Reject the intermediate `$tasks->heartbeat()` accessor/public hub object.
TaskGroup is the public hub/structured owner; periodic deadline bookkeeping can
remain internal. This confirms placement, not every remaining lifecycle policy.

Proposed vocabulary: a scope-owned heartbeat hub registers actions through
`every(float $seconds, callable $action)` and returns a per-registration control.
That handle can support local `setInterval()`/`stop()` and be passed explicitly to
its callback. Direct handles identify registrations without mandatory public names
or IDs. Registration handles are not the Awaitable for one invocation, and completed
invocations are never restarted; recurring work consists of fresh calls.

Possible API sketch, NOT LOCKED:

```php
$cache = $tasks->every(1.0, function (PeriodicControl $control): void {
    refreshCache();
});
$metrics = $tasks->every(5.0, function (PeriodicControl $control): void {
    publishMetrics();
});
$cache->setInterval(2.0);
```

Proposed integration: reuse the owning scheduler's monotonic timer/deadline and
task execution facilities, not a second event loop or a fixed-frequency polling
thread. A slow cache action should not force metrics to wait for its batch; actual
non-suspending PHP/native blocking still stalls the cooperative loop. Recommended
per-registration policy is one active invocation at most, missed deadlines skipped,
with independent progress between registrations. Stop forbids new invocations and
settles active ones cooperatively; it is not arbitrary task preemption.

Open gates: whether every registers and activates immediately or needs explicit
activation; reconcile the convenience API with the locked public spawn-only task
creation boundary (an explicit API extension needs agreement). Define internal
spawn usage, callback/context argument transport, same-scope external handle access,
timer replacement on setInterval during wait/run, first tick/cadence, failure policy,
hub-wide stop, closed-handle behavior, teardown/drain and resource bounds. Active
periodic registrations must never detach from their owning run scope. Do not claim
external controls or hidden task creation as already approved by this clarification.

Direct API direction: `TaskGroup::spawn()` submits one task; proposed
`TaskGroup::every()` registers recurring work at a per-registration interval.
Unlike the historical blocking every-loop proposal, registration should return a
control handle without occupying the registering task. Callbacks execute in
scope-owned scheduler tasks. This is a deliberate public API extension to discuss,
not a request to bypass structured ownership or create another event loop. Exact
return type/name, optional argument transport and control/context injection remain
to be specified. Scope inputs should be explicit callback arguments, not mandatory
captures; introducing automatic argument injection into spawn is not implied.

## Exploration — unified public scheduler facade with explicit execution mode

The maintainer proposes simplifying the API so a task's Async/Parallel domain is
selected by an enum or equivalent explicit mode instead of separate public call
surfaces. Treat this as an API redesign under discussion, not an implementation
instruction or an automatic replacement of existing LOCKED decisions.

Suggested direction to discuss: one run scope and TaskGroup; explicit Async/Parallel
mode on spawn and every; one task-result facade with a common await operation.
Keep the two execution engines and their shared-versus-isolated memory rules
distinct underneath. A same-name await must suspend the parent scheduler until a
worker settles, not disguise the current blocking Future::join implementation.
This requires the still-deferred completion/wakeup adapter and mixed-scope drain,
failure/cancellation and ownership contracts. A Parallel recurring control would
need worker-local command proxies, not a cross-arena PHP object.

Example spelling only, NOT LOCKED: spawn(Execution::Async, callback, ...args),
spawn(Execution::Parallel, closure, ...args), and
every(Execution::Parallel, seconds, closure, ...args). Mode-before-payload arguments
avoids stealing names from the task's named variadic argument payload. Decide
parameter order/defaults and names explicitly; do not add boolean mode switches,
automatic offloading, or promise arbitrary runtime-selected modes bypass transfer
validation. Existing explicit argument transport stays part of the design.

Reopen public namespace/placement and distinct-result-handle decisions deliberately
if this direction is selected. Decide legacy compatibility versus replacement,
static/dynamic mode checking, worker-context restrictions and heartbeat policy.
Current native heap/regression/review gates remain open and separate. No facade,
enum, unified result handle or heartbeat runtime code has been implemented here.

## Namespace choice — Elephc\Task

The maintainer selected `Elephc\Task` as the public namespace for the unified API:
the developer defines tasks, and the name aligns with TaskGroup. Retire the
assistant's provisional `Elephc\Concurrency` spelling. This fixes the namespace
of the target design, not implementation readiness or every signature.

Current design vocabulary: `Elephc\Task\run()` owns and supplies a TaskGroup;
spawn submits one execution and every registers periodic work with an independent
interval; an explicit Async/Parallel enum selects the execution engine. Execution,
Task/result-handle and PeriodicControl names remain tentative. No public
heartbeat accessor or separate hub object is required.

This targets a single public namespace instead of the existing Async/Parallel
surface; keep distinct internal engines, transfer checks and structured ownership.
Compatibility/migration and unified completion/cancellation/control contracts still
need consensus before code changes. Namespace selection is not authorization to
rename existing APIs, implement heartbeat or mark the current candidate accepted.
