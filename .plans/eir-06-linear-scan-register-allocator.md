# Register allocation: remaining interval-splitting work

- [x] Ship linear-scan allocation, liveness/interval analysis, scalar rematerialization, target register pools, and the stack fallback.
- [ ] Define position-specific allocation locations and split-child ownership.
- [ ] Split intervals at register-pressure and ABI-clobber boundaries; emit the required spill/reload transitions.
- [ ] Reconcile split locations across block arguments and critical edges.
- [ ] Add differential, structural, ownership and supported-target coverage before enabling the new representation.
- [ ] Update the permanent optimization/backend reference after implementation.

## Current baseline

Reviewed against release candidate `df57a9800f` on 2026-10-19. Linear scan is
already the default; `--regalloc=stack` and `ELEPHC_REGALLOC` select the fallback.
The historical statement that `src/ir_passes/` does not exist is obsolete.

`src/ir_passes/intervals.rs` constructs contiguous classic `[start, end]`
intervals. `src/ir_passes/allocation.rs` assigns one register, spill home or
constant recipe per `ValueId`; `location()` has no instruction-position
argument. `src/ir_passes/regalloc.rs` can evict an entire interval, but does not
implement the original plan's Task 6, interval splitting with explicit boundary
stores/reloads. The shipped allocator does not complete that milestone.

## Implementation boundaries

| Responsibility | Current source |
|---|---|
| Live-in/live-out and interval construction | `src/ir_passes/liveness.rs`, `src/ir_passes/intervals.rs` |
| Register pools, pressure and call clobbers | `src/ir_passes/regalloc.rs`, `src/ir_passes/clobber.rs` |
| Allocation result and scalar recipes | `src/ir_passes/allocation.rs`, `src/ir_passes/rematerialize.rs` |
| Backend consumers and frame homes | `src/codegen/value_placement.rs`, `src/codegen/frame.rs` |
| Edge materialization and cycle-breaking moves | `src/codegen/parallel_moves.rs`, `src/codegen/lower_term.rs` |

First specify how `location_at(value, position)` or an equivalent API identifies
locations before/after a clobber and on each edge. Preserve stable spill homes,
constant bit patterns, separate integer/float pools, PHP ownership, and the
existing use-weighted pressure heuristic. Splitting a range must not extend a
borrow or duplicate an owner.

All supported targets are first class: macOS ARM64, iOS device ARM64, iOS
Simulator ARM64, Linux ARM64 and Linux x86_64. Use shared target/ABI helpers.
Preserve x86_64 floating-point call-clobber rules and the conservative stack
policy for generators and functions with implicit exception edges until a
separate proof permits changing them.

## Acceptance

Require hand-built CFG tests and executable comparisons of linear versus stack
placement for high pressure, calls, loops, mixed integer/float values, parameter
swaps and edge cycles. Cover exceptions, cleanup and generator suspension, and
all five target emitters. Keep implementation validation focused; this planning
cleanup ran no tests. The permanent reference is
[optimization](../docs/compiling/optimization.md#register-allocation).
