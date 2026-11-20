---
title: "APM for every target: transactions outside HTTP, --with-apm, push to a collector"
author: Guillaume
date: 2026-11-16
branch: feat/monitor-apm
touches:
  - crates/elephc-probe/src/apm_runtime.rs
  - crates/elephc-probe/src/apm_runtime_tests.rs
  - crates/elephc-probe/src/endpoint.rs
  - crates/elephc-probe/src/lib.rs
  - docs/beyond-php/apm.md
  - docs/compiling/cli-reference.md
  - src/cli.rs
  - src/codegen/block_emit.rs
  - src/codegen/frame.rs
  - src/codegen/mod.rs
---

## Decided
- A transaction is one entry into compiled code: web request, CLI run (`CLI `), Messenger message (`MSG <class>`), library export call (`CALL <export>`), `--apm-transaction Class::method|function` (`TXN `). The page filters the table by these prefixes.
- Probe API `elephc_probe_txn_begin(name) -> token` / `_fail(token)` / `_fail_current()` / `_end(token, failed)`; a transaction opened inside another (or inside a web request: CURRENT_ROUTE/PENDING_LEN set) gets token 0 and counts nothing. fail and end ignore token 0 so a nested exception does not fail or close the outer one.
- Hooks are an AST rewrite after autoload::run (`src/apm_prelude.rs`): wrap body in begin/try/catch(fail, rethrow)/finally(end). Messenger: `Worker::handleMessage` named from `get_class($envelope->getMessage())`; the worker swallows handler exceptions, so `WorkerMessageFailedEvent::__construct` calls `_fail_current`.
- `--with-apm` = probe without instrumentation/sampler (`shared.apm_only`, `elephc_probe_apm_init`); `arm_timer` is a no-op unless the SIGPROF handler was installed (SAMPLER_INSTALLED) — arming without a handler kills the process.
- Library exports: the public symbol is a trampoline (`cdylib/apm.rs`) in front of the unchanged wrapper renamed `__elephc_apm_inner_<name>`; it saves x0-x7/d0-d7 (rdi..r9/xmm0-7), copies caller stack args, closes with BOUNDARY_STATUS != 0 as failed. elephc_init starts APM, elephc_shutdown flushes.
- Push (`crates/elephc-probe/src/push.rs`): mutual HMAC proof with domain strings distinct from the endpoint's, per-frame keys (no keystream reuse). Hello carries `delivered`: 0 => the collector counts the first snapshot whole (short runs), else it is a baseline (restarted collector). Final send at exit via `emitter.exit_hook = elephc_probe_apm_flush`.
- The endpoint calls the instrumentation's exact-capture entries through `elephc_probe_instr_capture` slots filled by emit_probe_init, so a `--with-apm` binary links without elephc-instr and answers an exact ask with "built with --with-apm".

## Tried and ruled out
- Instrumenting every exit path of the scalar/string export wrappers on both arches: the trampoline in front is one place.
- Sealing several frames with one session key: handshake::seal restarts its keystream per call, so each frame gets its own derived keys.

## Watch out
- The probe crate never built for iOS before; signal-context reading now keys on target_vendor = "apple".
- `monitor --apm` without a target used to mean "browse the store": `--listen` is exempted from the "needs a running service" checks.
- A hung `cargo test` of another session holding the shared build lock (~/.cargo/targets/elephc-5e671df1) blocks every build on the machine.
- A string-returning export's C signature appends the output pointer and length after the PHP arguments: the trampoline counts them among the stack words, or a host's stack-passed outputs are lost (SIGBUS in compose_label).
- Greptile round 4: a reset now holds the table gate (ROUTES_GATE_OFFSET, bounded waits) so no process writes between a lookup and the reset; CPU_AT_RESET_NS rebases the snapshot CPU; samples and events use route_of_this_generation; the run trend plots a folded minute's mean run.
