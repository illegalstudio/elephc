---
title: "Eval bridge inventory is emitted once per program, not per eval call site"
author: Guillaumeloulier
date: 2026-10-21
branch: perf/build-performance
touches:
  - docs/internals/the-codegen.md
  - src/codegen/lower_inst/builtins.rs
  - src/codegen/lower_inst/builtins/eval.rs
  - src/codegen/lower_inst/builtins/eval/constructor_params.rs
  - src/codegen/lower_inst/builtins/eval/constructor_registration.rs
  - src/codegen/lower_inst/builtins/eval/context_registration.rs
  - src/codegen/lower_inst/builtins/eval/function_params.rs
  - src/codegen/lower_inst/builtins/eval/function_registration.rs
  - src/codegen/lower_inst/builtins/eval/member_collection.rs
  - src/codegen/lower_inst/builtins/eval/method_registration.rs
---

## Decided
The eval bridge seeds a fresh eval context through ONE module-level helper (`src/codegen/eval_inventory_helper.rs`), not an inline copy per `eval()` call site. The status dispatch is likewise one helper (`src/codegen/eval_status_helper.rs`).

Both were inlined at first, which made assembly size quadratic in the number of eval call sites. `examples/eval/main.php` (381 lines, 169 eval sites) emitted 30,348,542 lines / 747 MB of assembly, 1,051,064 `__elephc_eval_register_native_method` calls, and took 378 s to compile (93 s of it in `as`). After hoisting: 132 MB, ~53 s on an idle machine, byte-identical program output.

Three constraints that each produced a wrong binary which still linked:
- The helper label must be GLOBAL. `emit_shared_helper` uses `emitter.label()`, which localizes; the helper is emitted in `finalize_user_asm`, after the bodies that call it. `emit_global_shared_helper` was added for this.
- The helper cannot use a frame local. The shared-helper prologue builds a fixed 32-byte frame, so a local at `x29-24` lands in the caller's stack. The eval context handle is parked in `_elephc_eval_inventory_context`, declared in `src/codegen_support/runtime/data/fixed.rs`, and the registration emitters take an `EvalContextSource`.
- `x19` is not free. Normal code carries the stack pointer in it (`mov sp, x19` in epilogues); a helper that clobbers it makes the caller restore a garbage `sp` (SIGBUS). The shared-helper prologue saves/restores it — do not remove that.

- A helper must not read the CALLER's eval scratch frame. Inside it `sp` belongs to the
  helper, so `EVAL_*_OFFSET` reads pick up unrelated bytes. Caller-frame values are passed as
  arguments — the status dispatch takes the pending Throwable in the first argument register.
  This one ran but raised an empty-class `Uncaught`; `test_eval_declared_function_throw_crosses_native_try_catch`
  catches it and passed on pristine `origin/main`, so it is a genuine regression guard.

Ordering also matters: the inventory helper must be emitted AFTER `emit_callable_argument_normalizer`, because registering the inventory sets `shared.callable_argument_normalizer` and the normalizer's own emission is gated on it.

## Tried and ruled out
- Writing into a separate `CARGO_TARGET_DIR` per config to A/B build profiles: the machine was shared with ~100 other concurrent sessions (load 90-113 on 18 cores), so wall-clock comparisons were dominated by contention. `--timings` per-phase numbers were the reliable signal.
- Giving the helper its own frame by extending `emit_shared_helper`'s prologue: every variant either overlapped the saved fp/lr pair, the nested-register slot, or left `sp` off by the 16 bytes the `stp` moves. The module-level symbol sidesteps frame math entirely.
- Using `rust-lld`/`ld64.lld` as the macOS linker: the toolchain's `ld64.lld` rejects the gcc-driver flags rustc passes, and `-C linker-features=+lld` is nightly-only. Not pursued further.

## Watch out
`examples/eval/main.php` is the canary for eval codegen size. `--timings` should show "Assembling object file" small relative to "Generating native code"; when it dominates, something is being emitted per call site instead of per program.

Disk state matters as much as code: the repo volume had reached 100% full (11 GiB free of 1.8 TiB) because `.claude/worktrees/` held 624 GB of accumulated `target/` dirs. Builds were competing for the last few GiB. Pruning worktree `target/` dirs and stale `target/debug/incremental` sessions reclaimed ~400 GiB.
