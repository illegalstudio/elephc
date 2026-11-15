---
title: Per-call context comes from the debugger runtime, not the monitor parser
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 90 }
touches:
  - crates/elephc-debug/src/lib.rs
  - src/monitor/call_html.rs
  - tests/codegen/cli.rs
  - docs/beyond-php/debugging.md
  - docs/compiling/cli-reference.md
  - .plans/native-debugger.md
---

## Decided

- Every call event now carries:
  - `caller: {file, line}` from the caller activation's `last_site`;
  - `timestamp_us` at entry and `end_timestamp_us` at return, measured from a
    debug-session `Instant` origin;
  - `fiber`: `0` for main, otherwise the running coroutine pointer from
    `ACTIVE_FIBER_CHAIN`;
  - `allocs`/`frees` read through the counter addresses that
    `elephc_debug_boot` receives from the generated prologue, and
    `alloc_delta`/`free_delta` at return;
  - `exception: {class, message, code, caught}` attached by the throw hook to the
    innermost open call event.
- The fields are emitted in every calls mode, including `--calls=none`, and are
  reused by the postmortem call ring. `--calls=none` only omits args/returns.
- `record_return` now closes the event even when values are off, so timing and
  churn are not lost in `none` mode.
- Calls mode arms every site flag and tracks each activation's last source site.
  `SOURCE_SITE_IDS` is a boot-time `Vec<bool>` so the per-line hook can tell a
  source site from a branch/path counter without taking the debugger lock.
- The Calls HTML panel renders a `call-meta` line: caller site, timestamp, fiber,
  alloc/free delta and an exception badge. The unified page embeds that panel, so
  the same context appears under the Profile/Calls shell without extra plumbing.
- `_gc_allocs`/`_gc_frees` are emitted into every generated runtime. The
  generated prologue passes their addresses to `elephc_debug_boot`; the debug
  crate never spells the hardcoded-underscore names itself, so a debug build
  without `--with-monitoring` still reports churn and every ELF link stays legal.

## Tried and ruled out

- Taking the debugger lock in the site hook to test `sites.contains_key` on every
  line: it serializes site capture against call recording. The immutable
  `SOURCE_SITE_IDS` bitmap is lock-free.
- Inferring the caller from the call-stream order: a thrown call has no return,
  and fibers interleave stacks; the caller activation is authoritative.
- Resolving timestamps in the monitor: the event must be self-contained for
  JSONL, postmortem and the HTML panel. `Instant` at boot gives a monotonic
  timeline that survives serialization.
- Reading wall-clock time per call: `clock_gettime(REALTIME)` is unnecessary for a
  relative call timeline and harder to compare across hosts.
- Declaring `static _gc_allocs: u64` directly in this crate: it links in a
  generated program but breaks the crate's own unit-test binary, because
  `_gc_allocs` is a hardcoded-underscore runtime name that only the emitted
  assembly can resolve on ELF. The boot-time pointer keeps the symbol in the
  assembly and still gives non-zero deltas in a plain `--debug` build.
- Emitting deltas only in `full` mode: allocation/free churn is mode-independent
  diagnostics. Only value rendering is mode-gated.

## Watch out

- Main's first call has no caller activation, so `caller` is absent; the HTML
  meta line still shows timestamp and `main`.
- A throwing call keeps an open event with `exception` but no `return` or
  `alloc_delta`; the test asserts exactly that.
- `timestamp_us` is microseconds since the debug session boot, not Unix time.
- The coroutine id is the runtime fiber pointer; it is stable within a run but not
  across runs.
- `--calls` now pays a per-line site hook to keep caller lines. This is a
  debug-capture cost; normal builds and profile-only runs do not arm site flags.
- The e2e fixture throws inside a caught fiber, so it covers caller line, fiber > 0
  and exception state in one run.
