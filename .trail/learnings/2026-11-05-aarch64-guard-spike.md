---
title: AArch64 guard spike proves low dormant cost and full ABI save/restore
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 80 }
touches:
  - tests/debugger_spike_aarch64.rs
---

## Decided

- A per-site guard with a load + conditional branch costs about **0.3–0.5 ns per
  iteration** on the AArch64 Linux host when the flag is zero.
- The active slow path — save x0-x17 and q0-q7, call a Rust hook, restore, read a
  scalar/string/array from `fp - offset` — costs about **5.6–6.3 ns per iteration**
  on the same fixture. This is a debug-build cost, not a production mode.
- The preserving wrapper survives a real Rust call with all tested caller-saved
  registers intact: x0-x17 and q0-q7. The first attempt overlapped the q-register
  save slots (144/160/176/192 for 128-bit pairs), which the ABI probe caught;
  the corrected offsets are 144/176/208/240.
- Frame-pointer-relative reads work for the metadata shape we need: scalar at
  `fp-16`, string pointer/length at `fp-32`/`fp-24`, array pointer/length at
  `fp-48`/`fp-40`.

## Tried and ruled out

- Saving only a hand-picked subset of caller-saved registers: the probe exists
  precisely to catch a missing pair. The v1 implementation can use the full set
  and optimize only after this evidence.
- Reading locals through the wrapper's own frame: the hook must receive the
  caller's `fp`, otherwise every offset is shifted by the wrapper frame.

## Watch out

- A Linux x86_64 counterpart now exists at `tests/debugger_spike_x86_64.rs`. It
  was executed under `qemu-x86_64`: guard, ABI and frame-local tests pass. The
  active 753 ns/iter is emulator-inflated and must not be compared with the
  native AArch64 numbers.
- The first x86_64 ABI run failed because `sub rsp, 336` changed the flags before
  `pushfq`. Save flags before the prologue (`pushfq` at function entry) and
  restore after `pop rbp` (`popfq`); do not capture them after a stack
  adjustment.
- `tests/debugger_spike_macos_aarch64.rs` is the Mach-O variant. It compiles to
  an object with `rustc --target aarch64-apple-darwin --emit=obj`; execution
  remains a macOS CI step. Mach-O needs `_` C symbols, `L` local labels,
  `@PAGE`/`@PAGEOFF`, and `__TEXT,__const` rather than `.rodata`.
- The q-register save area must step by 32 bytes per pair, not 16. The test now
  sets and checks all eight q registers.
- The fixture uses callee-saved x19/x20 (guard) and x19-x22 (ABI probe); those
  must be saved by the fixture itself, independently of the debugger wrapper.
- This is a spike, not the production emitter. Do not copy the hand-written
  assembly into codegen without routing it through `src/codegen_support/abi/`.
