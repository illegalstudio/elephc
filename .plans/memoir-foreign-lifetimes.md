# Memoir external ownership lifetimes

- [x] Verify the clean branch and current main ancestry.
- [x] Freeze original external-record, foreign-release and conditional-cleanup assembly.
- [x] Commit readonly fingerprints and all-target assembler checks.
- [ ] Extract publication of external records into owner_records.
- [ ] Extract protected foreign-owner release with explicit host hooks.
- [ ] Extract conditional extra-owner release with typed host layout facts.
- [ ] Validate unchanged output, context forwarding, native ownership and compiler-only boundaries.
- [ ] Update docs, remove task-owned tools and commit.

## Scope

Code Journal plan: 6e0015ad. Start at 13e62f2589 on main bc5b97bfd4.
C ABI symbols/versioning, borrowed Rust ownership policy, callback selection and
PHP assignment refusal remain compiler-owned. Memoir emits external record
publication, contained release through a boxed exception accumulator, and optional
extra-owner retirement. Native caller layouts and the extra-owner flag come from
the compiler. Gate implementation and publication are outside this phase.

## Verification

Freeze text and sorted internal labels before production changes. Five targets
and six modes cover external records (30 cases), normal and pin release (60), and
conditional release alone or through the full refusal path (60), 150 total.
Commit readonly snapshots and cross-assemble every case before extraction.
Require unchanged output afterward, explicit host-context tests and focused
native tests for eval teardown, pinned roots, coroutine records, throwing
destructors and nullable-reference refusal, together with boundary and hygiene
checks. Queue every compilation, assembly and executable test.

Readonly verification: three fingerprint tests and two cross-assembler tests passed
against unchanged production code at 13e62f2589, covering all 150 original cases.
