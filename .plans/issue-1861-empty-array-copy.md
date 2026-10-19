# Issue #1861: copied array storage

Base: `6459eb01d3439977e95acc50943f2e99b6927916`.

A destination that requires associative keys must convert every indexed RHS shape,
not only a literal. `ArrayToHash` consumes an input owner; retain borrowed and
provisionally owned concrete local loads so that the source binding survives.
Negative literal keys require associative storage at the checker, loop fixed point,
and EIR write site, including after element-type widening.

## Validation

All receipts stay under `.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/issue1861/`.
`final-artifacts.json` pins the final CLI, library, probe, runtime emitter and each
changed Rust source file. Its CLI SHA-256 is
`a55d23cd079118dbc64508b129d3567aa94a4b63d1d4b1ec35566478d853a4b2`.

- Exact-main baseline reproduces dropped negative keys and sparse-key gaps.
- 444 focused tests pass with each IR mode (888 total): associative arrays,
  ownership, array regressions, local retyping, and type annotations.
- Eight PHP-checked fixtures produce 80 program/runtime object combinations across
  macOS ARM64, Linux ARM64/x86_64 and iOS device/Simulator, with both IR modes.
- All 48 macOS/Linux executions match PHP 8.5 and report clean heaps. Linux binaries
  run in the architecture-specific Docker images; iOS evidence is compilation and
  assembly, not native execution.
- All 16 original-shape macOS cases and 14 copy/direct controls across profiles
  8.0 through 8.6 pass. Only PHP 8.5 is installed as a native oracle. The old-profile
  empty-array append cursor remains the separate #1494 / #1568 runtime change;
  these controls establish copy invariance, not full PHP 8.2 parity.
- Supplemental populated-object lifetime replay produces 1600 with clean heaps in
  both modes, including unsetting the source and allocating intervening strings.
- `git diff --check` passes; all touched Rust files retain their module preamble.

Candidates 1 and 2 were rejected for consuming provisional local owners. Their
logs remain alongside the final receipts and are not acceptance evidence.
