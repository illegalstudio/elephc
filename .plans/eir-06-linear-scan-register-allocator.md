# Phase 06 — Linear-Scan Register Allocator

> **For agentic workers:** Replace the "every SSA value gets a stack slot"
> placement from Phase 04 (the current production EIR backend) with a real
> linear-scan register allocator. This is the first significant performance gain
> of the EIR series.

**Status of the surrounding series:** Phases 1–5 are done. The EIR backend is the
**default**; `--ast-backend` is deprecated (removal in v0.26.0). The Phase 04
placement — every non-void SSA value gets a fixed stack slot, see
`src/codegen_ir/value_placement.rs` ("Phase 06 replaces this") — is the live
baseline this phase replaces. `src/ir_passes/` does not exist yet.

**Goal:** Implement linear-scan register allocation (Poletto & Sarkar, 1999) over
EIR functions, producing per-`ValueId` register/stack assignments. Scope is the
**full** algorithm: liveness + live intervals + active list with furthest-use
spill selection, **plus interval splitting and explicit spill/reload code**,
separate int/float register pools, and callee-saved usage (with save/restore)
for values that live across calls.

**Architecture:** New IR-level pass under `src/ir_passes/` (consistent with the
EIR overview: read-only analyses + sidecar tables, invoked from `pipeline.rs`
after lowering, before codegen; the later optimization phases 7–9 live here too).
Output is an `Allocation` table keyed by `ValueId`. The `codegen_ir` backend
reads `Allocation` and emits accordingly.

**Tech stack:** Rust, existing IR + ABI modules, no new dependencies.

## Rollout

The allocator is the **default**. A flag `--regalloc=stack` (style matches the
existing `--null-repr=sentinel|tagged`; default `--regalloc=linear`) forces the
Phase 04 spill-everything path as a fallback. This enables:

- **Differential testing**: the same `.php` must produce identical stdout under
  `--regalloc=linear` and `--regalloc=stack`. Primary safety net.
- **Instant rollback** if an allocation bug surfaces.

The flag and the legacy `stack` path are removed in a later cleanup once the
allocator is proven stable (fold into Phase 09 cleanup).

---

## Integration point: chokepoints only

The EIR backend centralizes operand materialization. Integration touches **only**
these helpers; the `lower_inst/*` emitters are **not** modified:

- `load_value_to_reg`, `load_value_to_result`, `load_string_value_to_regs`
- `store_result_value`
- `value_offset`

Emitters keep computing in the result/scratch registers. What changes:

- **operand load**: if the value is in a register, emit a reg→reg `mov`/`fmov`
  (or nothing) instead of a memory load.
- **result store**: move the result from the result reg into the value's
  assigned register (or store to its spill slot) instead of always storing to a
  fixed slot.

This keeps intra-instruction codegen identical while eliminating stack
round-trips for register-resident values.

## What is register-allocated in the first cut

Only single-word values: `I64`, `F64`, `Heap(ptr)`.

- `Str` / `TaggedScalar` (16-byte ptr+len pairs) and `IterStart` (64-byte
  iterator state) stay on stack slots. Avoids register-pair complexity; still
  satisfies the roadmap. Extendable later.

## Register pools, callee-saved, target differences

- **Reserved scratch registers (not allocatable)**: `x9`/`x10`/`x11` (AArch64),
  `r11`/`r10`/`rcx` (x86_64). Emitters that grab scratch keep working unchanged.
- **Cross-call values prefer callee-saved**: `x19`–`x28` / `rbx`,`r12`–`r15`.
  Track which are actually used; emit save/restore in prologue/epilogue (extend
  `frame.rs`). If none free, spill around the call.
- **Target-critical (supported-target policy)**: SysV x86_64 has **no
  callee-saved XMM**, whereas AArch64 has `d8`–`d15` callee-saved (low 64 bits).
  So **float values live across a call on x86_64 must spill**; on AArch64 they
  may use callee-saved float regs. Tested explicitly on both.

## Correctness-critical constraints

- **Ownership / GC**: cleanup paths that release `Owned` heap values must find
  the value wherever it lives. Release code consults `Allocation`, never assumes
  the stack slot. Highest-risk interaction → tests in `tests/codegen/runtime_gc/`.
- **Exceptions**: values live into an exception handler (`try`/catch) or across a
  point that can throw must reside on the stack or in callee-saved regs (a throw
  unwinds caller-saved state). First cut: **force-spill** values live within /
  across a try region.
- **Generators**: across a `GeneratorSuspend` the state lives in the generator
  frame → **force-spill** values live across a suspend.

---

## File Structure

- Create: `src/ir_passes/mod.rs` — module entry, pass exports
- Create: `src/ir_passes/liveness.rs` — backward dataflow, per-block
  live-in/live-out, per-instruction live ranges
- Create: `src/ir_passes/intervals.rs` — `LiveInterval { value, ir_type, start, end }`,
  interval construction over linear program order
- Create: `src/ir_passes/regalloc.rs` — linear-scan core: active list, expire,
  furthest-use spill, separate int/float pools, callee-saved tracking
- Create: `src/ir_passes/allocation.rs` — `Allocation` table consumed by backend
- Create: `src/ir_passes/resolve.rs` — block-param parallel moves + cross-edge
  location reconciliation, critical-edge handling
- Create: `src/ir_passes/tests/` — unit tests on hand-built EIR
- Modify: `src/lib.rs` — add `pub mod ir_passes;`
- Modify: `src/cli.rs` — `--regalloc=linear|stack` flag (default linear)
- Modify: `src/pipeline.rs` — run the pass per function after lowering, before
  codegen; thread the chosen mode
- Modify: `src/codegen_ir/value_placement.rs` / `frame.rs` — build `Allocation`,
  or fall back to spill-everything under `stack`
- Modify: `src/codegen_ir/context.rs` chokepoints — `load_value_to_reg`,
  `load_value_to_result`, `load_string_value_to_regs`, `store_result_value`,
  `value_offset` become allocation-aware
- Modify: `src/codegen_ir/lower_term.rs` — block-parameter moves consult `resolve`

---

## Staging (independently landable tasks)

Each task ends green on `cargo test`, `cargo test -- --include-ignored`, and the
Docker Linux scripts where target-sensitive. TDD: failing unit test first.

### Task 1 — Liveness analysis
Backward dataflow to fixpoint over `ValueId`: per-block `live_in`/`live_out`.
Block params are defs at block entry; instruction/terminator operands (incl.
branch args) are uses. Unit tests on hand-built functions (straight-line, loop,
diamond). Files: `mod.rs`, `liveness.rs`, `tests/liveness_test.rs`, `lib.rs`.

### Task 2 — Live intervals
Linearize blocks in reverse postorder; number positions. Build
`[def, last_use]` per value, extended across loop back-edges. Unit tests on
overlapping and disjoint ranges. Files: `intervals.rs`, `tests/intervals_test.rs`.

### Task 3 — `Allocation` + register-aware chokepoints in "all-spilled" mode
Define `Allocation` and make the backend consume it, but produce all-stack
placements so output is byte-for-behavior identical to Phase 04. Proves the
integration at zero risk. Differential test harness wired here. Files:
`allocation.rs`, `codegen_ir/context.rs`, `frame.rs`, `cli.rs`, `pipeline.rs`.

### Task 4 — Linear-scan core (assignment, no splitting yet)
Active list sorted by end; expire; assign from matching pool; furthest-use spill
to a slot (whole-interval spill). Separate int/float pools from
`codegen/abi/registers`. Pre-coloring/fixed intervals for ABI constraints
(incoming params, `Call` results in x0/d0, `Call` operands → arg regs, `Return`
value). Unit tests: fits-in-regs, forced spill (10+ live), mixed int/float.
Files: `regalloc.rs`, `tests/regalloc_test.rs`.

### Task 5 — Callee-saved usage + prologue/epilogue save/restore
Cross-call intervals prefer callee-saved; track used set; emit save/restore.
Handle the x86_64-no-callee-XMM float rule (force float spill across calls).
Tests on call-heavy functions, both targets via Docker. Files: `regalloc.rs`,
`codegen_ir/frame.rs`.

### Task 6 — Interval splitting + explicit spill/reload
Split a spilled interval at the conflict point: part in reg, part in slot, with
reload/store at the boundary. Furthest-use spill candidate. Files: `regalloc.rs`,
`intervals.rs`, chokepoint reload paths.

### Task 7 — Block-param resolution
Parallel moves at edges (cycle-breaking via one scratch reg), cross-edge location
reconciliation, critical-edge splitting. Files: `resolve.rs`,
`codegen_ir/lower_term.rs`.

### Task 8 — Correctness constraints: GC, exceptions, generators
Release paths consult `Allocation`. Force-spill values live within/across try
regions and across `GeneratorSuspend`. Files: `regalloc.rs`, ownership/cleanup
emitters. Tests in `tests/codegen/runtime_gc/`.

### Task 9 — Default switch + differential CI
Default `--regalloc=linear`. Differential test: full `codegen_tests` suite
produces identical stdout under `linear` and `stack`. Stress: high pressure,
calls, loops, float-heavy on x86_64.

---

## Exit criteria

- Linear-scan allocator integrated, default on, producing valid placements.
- Full suite green under both `--regalloc=linear` and `--regalloc=stack`, on all
  three targets (Docker Linux x86_64 / ARM64 + local macOS ARM64).
- Edge cases tested: spilling, interval splitting, float pool, callee-saved
  across calls (incl. x86_64 float-spill rule), block-param parallel moves,
  GC/exception/generator force-spill.
- `docs/internals/the-ir.md` gains a "Register allocation" section.
- `cargo build` clean, zero warnings.
