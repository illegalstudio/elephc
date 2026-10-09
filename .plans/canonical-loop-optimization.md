- [x] Share basic induction recognition with integer range analysis and simplify equivalent loop state and tests.
- [x] Hoist invariant loop bounds and materializations through LICM without speculating effects.
- [x] Thread SSA forwarding edges and merge single-predecessor loop update blocks.
- [x] Add structural, semantic, overflow, control-flow, and target coverage.
- [x] Update the optimizer documentation, example, and roadmap after focused validation.
- [x] Exercise coalesced checked counters at overflow through EIR assertions and native execution.
- [x] Pin the conservative non-Int I64 induction policy with focused unit coverage.

- [x] Reproduce and profile the widespread eval compilation timeouts from CI run 37912270881.
- [x] Fix the measured scaling regression and add focused regression coverage.
- [x] Validate affected tests and build hygiene before committing or pushing.

## Implementation

Recognize scalar header parameters with a common constant-step recurrence on every
back edge. Reuse this structural analysis for integer range proofs. Coalesce
induction variables with identical initial values, steps, and overflow semantics.
For checked recurrences, retain the dominating first check and require equal
conversion modes; integer range analysis still owns check removal. Replace
self-carried scalar header parameters with their preheader values. Canonicalize
integer loop comparisons with the induction variable on the left.

Extend LICM to move pure non-owning nullary materializations used by loop tests or
updates. Preserve mutable loads, allocations, exceptions, and zero-trip behavior.
Keep instruction placement and value definitions consistent after motion.

Compose edge arguments when threading forwarding blocks. Skip parameters that
escape their forwarding terminator. Merge single-predecessor blocks within the
same natural loop, preserving loop headers and preheaders so fixed-point cleanup
does not undo LICM. Preserve stable block and value identifiers with dead Nops.

Validate hand-built EIR and real PHP with optimization on and off. Include nested
loops, continue and break, descending counters, differing updates, overflow,
zero iterations, exceptional control flow, and assembly generation for all five
supported targets. Use focused tests and check compiler build diagnostics.

## Validation

- Linux x86_64 Docker: all 216 EIR pass unit tests passed.
- Linux x86_64 Docker: 439 optimizer integration tests passed, one existing test
  ignored. Includes explicit optimization-on/off behavior and assembly generation
  for Linux x86_64, Linux ARM64, macOS ARM64, iOS device, and iOS Simulator.
- PHP cross-checks confirm the example and semantic fixture outputs.
- `git diff --check`, module preambles, and new-file whitespace checks pass.
- `cargo build -p elephc --bin elephc` passes in the same Linux x86_64 container.
  Existing Alpine bridge warnings concern deprecated libc time aliases;
  follow-up is Code Journal task `f00ceb7c`.

## Review follow-up

Two exported PHP fixtures now require one shared counter and one remaining checked
update in optimized EIR, including the expected cast/fatal overflow mode. Explicit
cast recurrences cross the upper integer boundary on the first or second update;
both optimizer modes produce identical native values, output order, and exit status.
Their numeric outputs also match PHP with its existing cast warnings suppressed.
Raw-slot recurrences retain the first fatal before the intervening `print()`.

The raw-slot test pins optimized behavior only: the existing single-counter
fixture also terminates on overflow with optimization enabled but continues with
optimization disabled. This difference does not require counter coalescing and
is tracked separately in Code Journal task `86fff60e`.

A valid single-iteration Boolean fixture verifies that I64 storage does not grant
PHP Int induction recognition or coalescing. A companion test confirms comparison
range proofs can still remove a safe checked update without an induction summary.
The intentional metadata restriction is documented in the shared recognizer and
the EIR guide.

Focused Linux x86_64 Docker validation passed: 22 loop-pass unit tests, 16 integer
range unit tests, and nine loop-optimization integration tests. The cast fixture
also emits checked arithmetic for all five targets with optimization on and off.

## CI performance follow-up

The initial CI head timed out across eval-heavy tests. A minimal opaque eval
program took 71.09 seconds to emit assembly in a local debug build: 53.57 seconds
in EIR optimization and 16.27 seconds in code generation. Per-pass profiling
located 41.80 seconds in dead-instruction elimination, primarily in injected
DateTime parsing methods after LICM expanded their loop live sets.

Replace repeated full liveness sweeps with a predecessor worklist that sends only
newly live values. Preserve the same definitions, upward-exposed uses, terminator
arguments, and public live-in/live-out sets. Add coverage for a large loop with
reversed block layout, loop parameter kills, parallel edges, joins, and unreachable
uses. Keep all CI timeout budgets unchanged.

The same fixture emits identical EIR and assembly after the repair. Assembly
emission takes 25.48 seconds (14.12 seconds optimizing EIR and 10.10 seconds
generating code), versus 71.09 seconds before. All 235 EIR pass unit tests pass,
as do 441 optimizer integration tests with one existing ignored test. Nine previously timed-out CI cases also pass with their existing budgets, with
two tests running concurrently in the 2-CPU container:

- Two mem2reg eval cases: 52.59-52.64 seconds against 60 seconds.
- Array append and strict-PHP eval cases: 36.35-36.37 seconds against 60 seconds.
- Two PHP profile cases: 143.43-143.77 seconds against 300 seconds.
- Three OPcache cases: 70.80-109.18 seconds against 180 seconds.

`cargo build -p elephc --bin elephc` and `git diff --check` pass. Temporary
profiling instrumentation was removed before these tests. No CI timeout budget
was changed. The earlier baseline and repaired stress-test binaries ran the
same reversed-layout fixture in 38.38 and 0.46 seconds respectively; those
microbenchmark runs shared the container with a Rust build, so the isolated
end-to-end measurement above is the primary performance comparison.
