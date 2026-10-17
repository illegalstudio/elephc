# Nullable scalar coalescing — issue #1835

- [x] Refresh open issues/PRs and confirm no open PR covers this issue.
- [x] Create a clean dedicated worktree inside Elephc from current origin/main.
- [x] Reproduce nullable scalar coalescing against PHP and freeze the baseline.
- [x] Preserve the semantic nullable type and its ABI through lazy merge locals.
- [x] Cover null/non-null int, string, float and bool values, `?:` and `??=`.
- [x] Replay object/callable controls, focused ownership tests and supported targets.
- [x] Update internals documentation and save the tested decision in Trail.
- [x] Run hygiene/Trail checks, refresh the base and prepare a single commit and fork PR.

## Scope and baseline

Base: d3542732744e46dbe623951d53ee0ddba4f2498e. The issue reports scalar null
values being stored in non-null merge slots after `?? null`. Preserve existing
object/callable behavior and PHP short-circuit/evaluation semantics. Do not fold
or rewrite unrelated AST nodes, runtime ownership or other worktrees.

All worktrees, generated probes, build outputs and temporary directories stay
inside the canonical Elephc project. Use one heavyweight Cargo writer at a time;
root dev artifacts are rebuilt when switching source worktrees. Exact compiler
hashes, PHP outputs, failed baseline and final results live in the ignored
maintenance directory. Focused tests and target evidence are separate gates;
no full local suite is planned.

## Reproduction and integration

The frozen d354273274 baseline fails 11 of the first 12 cases; its object/callable/array
control passes. The expanded baseline also fails inferred returns, static-property
assignment, declared scalar null-return checks and array coalescing assignment. PHP
oracles compare complete output; intermediate artifacts and failures remain preserved.

Rebased onto 259877ca617f0085391880c28f7f797fb81855fd after PRs #1822, #1848
and #1849 merged. The regression set covers lazy fallback/chaining, integer endpoints,
falsy and nullable scalars, local/instance/static/indexed/associative assignments,
inferred and declared returns, and repeated owned strings. Validation is complete; this plan records the tested change prepared for fork publication.

## Final validation

The final compiler SHA-256 is d09ff30e679792c00069abdd58b28e571e65021f3e9800c224b470f646aa5f66.
The tracked diff and regression source hashes match the frozen build receipt.

- Warning-free locked dev build with one Cargo writer and Rust debug symbols disabled;
  debug assertions remain enabled.
- 17 new regressions, 17 historical coalesce tests and 12 ownership/alias controls pass
  with IR optimization both on and off: 92 executions.
- 93 focused unit tests pass: effects 37, return aliases 13, shared detection 31 and
  PHP profile sensitivity 12. Existing pure method summaries remain unchanged.
- 188 program/runtime combinations assemble on all five supported targets, using 10
  feature-specific runtime objects from the exact current library.
- 20 PHP fixtures (including three function-scope variants) pass in both modes on macOS
  ARM64, Linux ARM64 and Linux x86_64: 120 executions with exact stdout and clean heaps.
  iOS device/Simulator evidence is compilation and assembly, not device execution.
- Rust module preambles and all 56 raw assembly instruction comments in the touched
  predicate emitter are present; comments remain aligned at column 81.

Trail's contextual TypeError/ownership and generic-control matches are lexical overlaps.
The existing rules remain in force: output values and owner lifetimes are checked as well
as final heap counts, and generic folding controls are preserved. The new learning records
these boundaries. A frozen CLI run needs an explicit crypto bridge path when relocated;
that harness error and intermediate failed artifacts remain in the maintenance directory.

Review follow-up: preserve implicit scalar return guards in direct/aliased/arrow closure effect and exception summaries. Rebased on b56a0c8cf3; 21 PHP codegen cases and 17 harness controls passed in each IR mode (76 focused test executions). The new direct/aliased/arrow closure guards throw TypeError for all four scalars and leave clean heaps. The review follow-up changes shared AST analysis; CI will replay the full supported-target matrix after publication.

Second review follow-up: first-class named calls now consult the same entry/return boundary-check set as direct calls. A new four-scalar regression covers literal and assigned function descriptors plus a valid nonthrowing result; the full focused replay passes in both IR modes.
