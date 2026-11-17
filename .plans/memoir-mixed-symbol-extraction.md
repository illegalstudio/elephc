# Memoir Mixed symbol publication

- [x] Verify the clean branch and current main ancestry.
- [x] Freeze original publication output for all targets, modes and ownership variants.
- [x] Commit readonly fingerprints and target assembler checks.
- [ ] Move publication behind an explicit typed host context while keeping the compiler adapter signature.
- [ ] Validate unchanged assembly, native/eval static-reference ownership and compiler-only boundaries.
- [ ] Update documentation, remove task-owned tools and commit.

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
