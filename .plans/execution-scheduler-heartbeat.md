# Scheduler heartbeat discussion

## Checklist

- [ ] Discuss developer use cases with the maintainer after stabilizing the merge/tests.
- [ ] Distinguish scheduler observability, worker liveness and application periodic work.
- [ ] Decide whether any public surface is warranted; keep it absent until consensus.
- [ ] Specify monotonic timing, scope ownership, cancellation and teardown if retained.
- [ ] Validate dormant cost, event volume and behavior when PHP or native code blocks.

## Request and preliminary framing

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
