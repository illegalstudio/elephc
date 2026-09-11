---
title: "Runtime Context Register (spike)"
description: "Decision note for the ctx-register spike: reserving x28/r14 for per-context runtime state, measured cost, fiber trap, and the M0 migration path."
sidebar:
  order: 30
---

**Source:** `src/codegen_support/runtime/ctx.rs` — layout, register convention,
`__rt_ctx_init`, and ctx-relative access helpers.

## Why a context register

The runtime keeps mutable per-process state in global `.comm` symbols:
`_heap_off`, `_heap_free_list`, `_heap_small_bins`, `_concat_buf`,
`_concat_off`, plus exception, fiber, and stack-guard words. A single
`--rt-ctx` build routes that state through one reserved register — `x28` on
AArch64, `r14` on x86_64 — pointing at a `_rt_ctx` struct. This is the
foundational refactor for the sandbox-threads plan: a second execution context
(a spawned thread in M1) only needs a different register value, not a second
copy of the runtime.

The flag `--rt-ctx` (CLI → `RuntimeFeatures::ctx_register`, cache-key bit 12)
selects the mode per build; the legacy symbol-addressed runtime remains the
default and both modes coexist in the same compiler.

## The layout

`_rt_ctx` places the small scalars first and the 64 KiB concat scratch last:

| Offset | Field |
|---|---|
| 0 | `_concat_off` |
| 8 | `_heap_off` |
| 16 | `_heap_free_list` |
| 24 | `_heap_small_bins[4]` |
| 56 | `_concat_buf` (65536 bytes) |
| 65592 | (16-byte aligned total) |

Scalar offsets stay within the AArch64 unsigned-imm12 window so every access
is a single `ldr/str xN, [x28, #imm]` — no intermediate address
materialization, unlike the legacy `adrp` + `add` pair per symbol.

## Register choice

`x28`/`r14` were chosen over the alternatives:

- **x18** is reserved by Apple's AArch64 ABI and never usable.
- **TLS** requires hand-emitting TLV descriptors (Darwin) or `.tbss`+GOT
  indirection (ELF) in the runtime object, adds a load per access, and fights
  the runtime cache's plain-symbol identity. Rejected for the register's zero
  measured cost.
- Both registers are callee-saved, saved and restored whole by
  `__rt_fiber_switch`, and are now excluded from the linear-scan allocator
  pools (AArch64 pool drops to x21–x27; x86_64 already only allocated rbx).

On x86_64, reserving `r14` also required migrating every hand-written runtime
helper that scratched it (~120 uses across 12 files) to `rbx`. That migration
moved the scratch traffic onto the ONE x86_64 callee-saved register the
linear-scan allocator assigns to cross-call values — an unsound footgun the
round-2 review caught (NB1). The final contract: every runtime helper that
touches `rbx` MUST preserve the caller's value (push/pop pair or a frame
spill slot, one restore per return path), enforced mechanically by
`x86_64_runtime_helpers_that_scratch_rbx_preserve_it`, which scans the full
emitted runtime for exactly this balance. The fiber wrapper's descriptor
scratch moved to `r15` with a descriptor reload, and the x86_64 callback
trampolines re-publish the ctx pointer before reaching compiled PHP code
(see Foreign entries below).

## Binary size

`_rt_ctx` is a `.comm` symbol, like every other runtime global. Emitting it as
`.space` inside `.data` — which the first version did — writes all 64 KiB of
zeroes into the image: a small program went from 70 KB to 136 KB. As a common
symbol the ctx build is 80 bytes SMALLER than its legacy twin, because it drops
the `_concat_off`/`_heap_off` globals it no longer needs.

## Measured results (macos-aarch64, host build)

An allocation-heavy program (4M array/string allocations + a 200k-entry hash
build), 5 alternating runs of the same compiled binaries:

- legacy: 1.41–1.44 s
- `--rt-ctx`: 1.33–1.40 s

The ctx mode is equal-to-slightly-faster (the imm-offset load replaces an
`adrp+add` pair on the hot alloc/free paths). **Cost of the reserved register:
in the noise.**

The pool reduction (8→7 callee-saved AArch64) was then measured on its own,
since it is paid by LEGACY builds too. Two spill-heavy programs, each built
twice from the same tree with x28 in and out of the pool, alternating runs:

- ten locals updated across a call in a 40M-iteration loop: 1.325 s vs 1.315 s
- eight distinct values live across calls, 5M iterations: 0.670 s vs 0.675 s

Both differences are inside the run-to-run spread, so x28 stays out of the pool
in BOTH modes and the two builds keep one register discipline. Worth knowing
what this does NOT prove: in the first program the allocator kept only three
values in callee-saved registers and spilled the rest, so the eighth register
was never the constraint. The second was written to put eight values in flight
precisely to remove that doubt.

## The fiber trap (found by the spike, fixed)

Generator and Fiber bodies run on freshly-mmap'd stacks whose fake initial
frame is deliberately zeroed, so the *first* `__rt_fiber_switch` into a
coroutine restores `x28 = 0` — and every allocation inside the body faults at
address `0x28`. The fix: `__rt_fiber_entry` re-publishes the `_rt_ctx` pointer
before any user code runs on the fiber stack. This is a permanent contract of
the ctx mode: **every entry point that adopts a fresh stack must re-publish
the ctx register.** The e2e test
`test_cli_rt_ctx_fibers_and_generators_re_publish_ctx` locks this behavior.

## Partial routing is incorrect routing (also found by the spike)

The first routing iteration only ctx-gated `__rt_heap_alloc`. The bench then
exhausted an 8 MB heap that the legacy build served fine: frees landed on the
*global* `_heap_free_list` (still read by nothing), and decref range checks
compared against the *global* `_heap_off` (permanently zero) so blocks were
never released. A ctx runtime where any heap-family helper still reads a
global is silently broken — recycling, refcount range checks, and the GC
walkers must all agree on the same state. The M0 migration must therefore be
complete-per-family, not incremental-per-helper.

This hazard is now mechanically enforced: ctx builds omit the legacy
`_heap_off`/`_heap_free_list`/`_heap_small_bins` symbols from the runtime data
section entirely, so any helper that still materializes them fails the link
with an undefined-symbol error instead of corrupting state at runtime.

## Foreign entries must re-publish (spike review, B2)

The fiber trap generalizes: the ctx register is callee-saved, so a host that
calls into compiled code preserves ITS value — pointing at host data, not at
zero and not at `_rt_ctx`. Every entry that reaches compiled PHP code from
foreign context must re-publish the pointer first (publish-only; never reset
allocator state mid-flight). Currently published at:

- the executable main prologue (full `__rt_ctx_init`),
- `__rt_fiber_entry` (zeroed fiber stacks),
- every cdylib/staticlib exported-function boundary wrapper,
- `elephc_init` and `elephc_shutdown` — the library lifecycle entries. The
  publish is the FIRST instruction after the prologue, ahead of the concat
  reset those entries perform: that reset is itself a ctx-relative store, and
  publishing after it stored through the host's register (a wild write that
  faulted at address 0 on the very first `elephc_init` of every ctx cdylib),
- extern FFI callback trampolines (called by foreign code like `qsort`).

Publishing is only half the contract. The ctx register is **callee-saved**, so
an entry called by a host also owes that host its register back: every one of
these boundaries spills the incoming value to a frame slot before the publish
and restores it on EVERY return path, error and exception returns included.
Without the restore the host gets elephc's `_rt_ctx` pointer in place of its own
value — an ABI break no PHP-level test can see, so it is pinned by a C host that
parks a sentinel in the register across the call
(`test_rt_ctx_cdylib_export_preserves_the_hosts_ctx_register`; the probe is
module-level assembly because clang spills an explicit `register … asm("x28")`
around the call and reloads it, which makes the C-level version of that test
pass against a library that clobbers the register).

The exception path is the documented exception: a throw unwinds via longjmp PAST
the trampoline, either to an enclosing PHP handler (the foreign caller never
resumes) or to the uncaught path, which exits the process.

## Scratch audit (spike review, B3)

A hand-written helper that starts using the ctx register as an ordinary
scratch register would silently corrupt the state pointer, and no other test
would catch it. The audit scans the ENTIRE ctx-mode runtime text — emitted with
ALL features on — and fails on any ctx register reference outside the
sanctioned shapes (publish sequences, ctx-relative accesses, whole-register
save/restore pairs).

Two tests share one scanner, both zero tolerance:
`aarch64_ctx_runtime_never_scratches_the_ctx_register` and
`x86_64_ctx_runtime_never_scratches_the_ctx_register`.

The audit's first version filtered lines on the target's comment prefix, so it
scanned comment text instead of instructions and reported zero offenders on
both targets — while `__rt_wordwrap` kept its output cursor in x28 and 81
x86_64 instructions still borrowed r14. **An audit that cannot fail is not an
audit**: the negative control matters as much as the assertion.

Where a borrowing helper goes depends on what it has left:

1. a free register that the allocator never assigns (`r15`, or any caller-saved
   one in a leaf helper) — a rename, nothing else;
2. `rbx`, with the caller's value preserved (push/pop or a frame slot), which
   `x86_64_runtime_helpers_that_scratch_rbx_preserve_it` then enforces;
3. a frame slot, when every register is spoken for — `sprintf`'s sequential
   argument cursor, `usort`'s length snapshot, `wordwrap`'s lastspace;
4. no register at all: two of the `getX`/wrapper helpers only compared the
   value once, and a memory-operand `cmp` replaced the load/compare pair.

A frame slot must keep the frame a 16-byte multiple: an 8-byte spill added to
an already-aligned frame is exactly what
`every_x86_64_runtime_call_site_is_sysv_aligned` fails on.

## Current state and what M0 must finish

Both targets route their whole heap and concat families through the reserved
register (alloc, free, heap_free_safe, incref/decref/heap-kind/GC range checks,
the heap-debug validator, descriptor release, object-handle and wrapper-cast
checks, the web arena reset); main installs the pointer via `__rt_ctx_init`;
fiber entry, both library lifecycle entries and every export/callback wrapper
publish it and hand the host's value back. `--rt-ctx` executables and cdylibs
compile, link, run, recycle their heap, and survive generators and fibers.

No helper on either target borrows the ctx register any more, and a
linux/amd64 container runs the same probe as macos-aarch64 with identical
results in both modes.

Still on the M0 work list:

- `_rt_ctx` as an emitter-shaped pool (array + free list) instead of a single
  instance, and `__rt_ctx_init`/`__rt_ctx_destroy` exported for the M1
  thread-pool bridge.
- The state families still on globals: exceptions, fibers, the GC counters,
  the ob/print_r buffers. Those are what M1 actually needs.

The full generated-runtime gate tests
(`ctx_feature_generates_ctx_addressed_runtime_end_to_end`,
`legacy_feature_keeps_symbol_addressed_runtime`), the scratch audit, and the
CLI e2e tests (`test_cli_rt_ctx_*`, including heap recycling and the
legacy-vs-ctx golden cross-check) pin both modes.