# Memoir Mixed symbol publication

- [x] Verify the clean branch and current main ancestry.
- [x] Freeze original publication output for all targets, modes and ownership variants.
- [x] Commit readonly fingerprints and target assembler checks.
- [x] Move publication behind an explicit typed host context while keeping the compiler adapter signature.
- [x] Validate unchanged assembly, native/eval static-reference ownership and compiler-only boundaries.
- [x] Update documentation, remove task-owned tools and commit.

## Scope

Code Journal plan: 8dc60d2e. Start at f768abafa9 on main bc5b97bfd4.
The compiler selects the static symbol, initialization-marker offset, displaced
owner release policy, optional transferred-owner slot and label prefix. Memoir
publishes the new owner, follows an existing persistent wrapper, clears the
transferred parking slot and initialization marker, then releases the displaced
owner when requested. Static-property type validation and EIR ownership decisions
remain compiler-owned. Gate implementation is outside this extraction.

## Verification

Freeze emitted text and sorted internal labels before moving code. Five targets,
six output modes, two prior-owner release policies and three transfer variants
(None, local offset 32, local offset 4096) produce 180 cases. Cross-assemble all
cases before and after extraction. Validate explicit host layout/prefix inputs
and ordering before reentrant release, plus focused direct, Reflection and eval
static-store cases with heap accounting, aliases, COW and throwing destructors.

## Original verification

Jobs af77c96a and 69e3c66a captured and verified 180 original fingerprints and
assembled every variant before production changes. The capture hook was removed
before committing; readonly fixtures include internal-label bookkeeping and use
no compiler-owned fixed data.

## Results

- Snapshot commit 6c2ac4a301 captured all 180 original publication variants before
  production extraction. The readonly fingerprint and cross-assembler tests passed
  on the original code and again after extraction, without fixture regeneration.
- Memoir owns the publication sequence through an explicit Context. RecordOffset
  identifies host initialization metadata and optional FrameOffset identifies the
  transferred owner slot. Both direct and retained eval callers are unchanged.
- The host-context test covers every target, mode, release policy and owner-slot
  variant with a different symbol, label prefix and marker offset. It verifies
  transferred ownership is cleared before initialization and initialization precedes
  displaced-owner destruction; disabling release emits no implicit decref.
- Final queued job 531ccf74 passed a warning-free compiler build and 16 focused Rust
  checks: four compiler tests, four contract tests, two compile-fail examples and
  six native tests. These cover direct/Reflection owner transfer in both IR modes,
  reentrant destructors, static COW, callback throws, lazy reflection reads and an
  opaque-eval static mutation bundle with balanced heaps.
- Cargo/source boundary audit, its 24 Python regressions, Rust preambles/function
  documentation, assembly comments and whitespace checks passed. README and memory
  model docs describe the final boundary. Task-owned tools and assembler scratch
  artifacts were removed. Main remained at bc5b97bfd4.
- Gate implementation, PR publication and complete remote CI remain separate work.
  This turn performed no push or PR creation.
