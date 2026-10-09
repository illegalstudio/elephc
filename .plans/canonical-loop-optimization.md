- [x] Share basic induction recognition with integer range analysis and simplify equivalent loop state and tests.
- [x] Hoist invariant loop bounds and materializations through LICM without speculating effects.
- [x] Thread SSA forwarding edges and merge single-predecessor loop update blocks.
- [x] Add structural, semantic, overflow, control-flow, and target coverage.
- [x] Update the optimizer documentation, example, and roadmap after focused validation.

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
