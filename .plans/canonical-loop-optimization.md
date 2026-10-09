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
- [x] Reproduce the five remaining multi-compile eval timeouts from CI run 37925824323.
- [x] Validate the scoped CI configuration repair before committing and pushing.
- [x] Audit every test shard in CI run 37925824323 before publishing the fixture-budget repair.
- [x] Profile compiler time and generated-program performance against main.
- [x] Restrict standalone LICM materializations to blocks dominating every latch.
- [x] Verify conditional and multiple-latch regressions plus all target emitters.
- [x] Remove the two profiling worktrees and branches with user authorization.

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


## Remaining CI fixture budgets

CI run 37925824323 at `abeef56f` reduced the original widespread failures to
five multi-compile eval fixtures in three Linux x86_64 codegen shards. Every
failure was the 60-second outer timeout on both attempts. These same fixtures
already receive the 180-second eval budget on Linux ARM64 and macOS.

Extend the existing Linux family override to x86_64 in both nextest profiles.
Keep fixture sources, repetition counts, ownership assertions, the default
60-second timeout, and the independent compiled-program watchdog unchanged.

Validate with the exact Linux x86_64 archive from that CI run, its Ubuntu CI
image and nextest 0.9.140, in a container capped at 2 CPUs and 12 GiB. With
profile `ci`, two concurrent tests, and zero retries, all five pass:

- Disabled output handler: 99.752 seconds.
- Constructor Mixed references: 100.075 seconds.
- Constructor temporary arguments: 118.364 seconds.
- Native eval argument-array keys: 118.227 seconds.
- Invalid Mixed throw ownership: 104.159 seconds.

The run takes 322.557 seconds overall. TOML parsing, nextest configuration
loading, and `git diff --check` pass. This is a scoped test-budget correction;
no compiler implementation or test assertion changes in this follow-up.

All remaining macOS shards subsequently passed on `abeef56f`, including the
last eval shard at 13:59 UTC. The completed test matrix has only the three
Linux x86_64 shard failures containing the five validated timeout fixtures.

## LICM profitability follow-up

Comparison with main `5d96278997` found that unconditional standalone constant
hoisting still inflated DateTime parser live sets: maximum live-in size rose
from 14 to 456. Seven counterbalanced native measurements pinned to one CPU
confirmed slowdowns of 17.4% for a one-iteration dispatch and 22.8% for the same
zero-trip loop on Linux x86_64. Profiling evidence and reproducible fixtures are
archived in Code Journal doc `57330abc`.

Require the defining block of a standalone materialization to dominate every
loop latch. Preserve invariant computations, their nullary dependencies, and
proven-immutable local loads. Compute the eligible block set once per loop.
The measured selective-hoisting experiment restored release EIR optimization
time to baseline and removed the dispatch regressions. Add structural coverage
instead of timing thresholds so CI detects excess hoisting deterministically.

Both clean profiling worktrees and their branches were deleted with `ggw`
after explicit user authorization. The feature worktree remains in place.

The two new structural tests and the 32-branch end-to-end regression fail on
the published LICM implementation because conditional constants reach the
preheader. With the repair, all 237 EIR pass unit tests and 442 optimizer
integration tests pass, with one existing ignored test. The dispatch fixture
checks zero, one, 32 and 64 iterations in both optimizer modes, matches PHP,
and emits assembly for all five supported targets. A separate Rust compiler
build completes without warnings; `git diff --check` passes.

A final isolated comparison against the previous PR compiler uses the committed
dispatch fixture inside 20 million calls, with length supplied through argv.
Seven alternating native runs pinned to CPU 4 reduce the zero-trip median from
0.3990s to 0.1927s and the one-iteration median from 0.4994s to 0.3400s. LICM
hoists 3 integer constants in that function instead of 66. Three alternating
opaque-eval compilations reduce the debug assembly-emission median from 21.220s
to 17.857s (15.8%). These compare the actual repair with the prior PR head,
not with main; the earlier main-baseline experiment is recorded above.
