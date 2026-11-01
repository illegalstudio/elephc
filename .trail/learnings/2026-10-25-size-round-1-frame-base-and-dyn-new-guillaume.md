---
title: "Binary size round 1: AArch64 frame-base register, dynamic-new helpers"
author: Guillaume
date: 2026-10-25
branch: reconcile/dirname-symfony
---

## Decided
- AArch64 frames whose slots reach past `ldur`'s 255 bytes keep x28 = x29 - (frame_size - 16) (`Emitter::frame_slot_base`); `load/store_at_offset` then emit one `ldr/str [x28, #off]`. x28 left the register allocator's pool and is saved/restored with the callee-saved registers. Symfony web 225.5 -> 210.9 MB, console 290 -> 262 MB with the two dyn-new changes.
  - Reset in `abi::emit_frame_prologue` (a nested helper never inherits an outer base), re-established at every block in `emit_blocks` (an inline `return` restores the callee-saved registers mid-body), cleared after the blocks, taken away in the three borrowed-frame callbacks (exception cleanup, destructor cleanup, backtrace frame reader).
  - `setjmp`/`longjmp` and the fiber switch save x19-x28, so catches and fibers keep a correct base.
- Dynamic `new $c(...)` helpers key every operand as boxed Mixed (already-boxed operands' PHP types split identical helpers), and the class arms allocate from the class template (`allocation_clone::emit_class_instance_allocation`, shared with the fixed-class `new`).

## Tried and ruled out
- Merging the 5 256 constructor padding thunks (one per class x missing arity, 7.5 MB) into one per class: needs garbage-tolerant parameters or a new calling convention; deferred.
- Frame zeroing (`stp xzr`) and `cbody` shared invoker bodies: already small / already deduplicated.

## Watch out
- `CARGO_BIN_EXE_elephc=<frozen compiler> cargo test ...` runs a test against an older compiler: the way to tell a regression from a pre-existing failure.
- Test campaigns from several worktrees saturate `syspolicyd` (400%+ CPU) and every spawned binary hangs at 0% CPU: a "timed out after 60s" with empty output is that, not the change.
