# Issue #1736: inherited native interface contract review

Reviewed #1758 at `5f5cfc54f63b523341cd8a5c6045a340bd668e0e`.
Both original patches rebased unchanged onto `62cfeb85efb1d7bbcd149c528a8bc97d959fdbde`.
The new fix reuses the shared signature compatibility primitives after dispatch flags
are checked, with the actual native declaring class and interface owner as type scopes.
Builtin methods without a registered signature retain their metadata-only fallback.

## Review addressed

`PRRT_kwDORs6r_s6r1YUM`: independent AOT methods are not inherently compatible with an
interface introduced later in eval. Return type, required arity, by-reference mode,
parameter types, relative return scopes and staticness now participate in validation.
Legal static, by-reference, optional, variadic and contravariant implementations pass.
The original alias/native-parent/Throwable fixes remain included.

## Evidence

- Warning-free compiler/default Magician builds from this worktree, with local Cargo
  entrypoints refreshed to avoid shared-target path-relative fingerprint reuse.
- 50 focused Magician interface unit tests and 10 Throwable tests passed, including
  rejection of a plain eval class implementing Throwable contracts.
- Four original PR controls passed: RuntimeException plus a Throwable interface,
  two eval ancestors, direct native method ownership, and a throwable parent alias.
- All 13 new individual native regression tests passed (278.94 seconds with two workers).
- Frozen aggregate contains 13 PHP-oracle cases. Baseline accepted incompatible return,
  arity, reference and parameter contracts and rejected a legal static method. The fixed
  aggregate makes all 13 expected decisions; accepted cases are heap-clean. Fatal
  declaration rejection is compared by outcome, not by PHP's detailed fatal wording.
- Final aggregate IR-off replay also matches all 13 decisions; accepted cases are heap-clean.
  Together, the two final modes cover 26 native declaration executions.
- No full local suite, Linux execution or iOS execution claimed. The implementation is
  shared Rust validation with no target-specific lowering or runtime ABI changes; CI
  supplies the remaining target coverage after publication.

Reproduce new regressions with:

```sh
cargo test -p elephc --test codegen_tests eval_inherited_interface_contracts
cargo test -p elephc-magician --lib interface
```

Native controls, complete stdout/stderr, frozen baseline executable/archive and hashes,
PHP outcomes, range-diff and the discarded initial replay are kept in
`/private/tmp/elephc-1758-review/`.

## Invalid evidence and separate follow-up

The initial per-fixture baseline replay was perturbed by an automatic bridge rebuild
while the source was briefly edited. It was explicitly discarded. Source was restored,
then the replacement baseline executable and archive were frozen before the fix build.

A switch-based aggregate witness hit an existing AArch64 conditional-branch fixup range
limit on the unchanged baseline. The source and assembler log are retained for a
separate main-branch bug fix. The contract witness uses an array of source strings;
no assembler/backend change is mixed into this PR.

## Publication

Publish only to the existing fork branch, with an exact-tip lease after squashing.
RustSec currently flags rustls 0.23.45 already present on main; #1843 separately updates
it to 0.23.46. That dependency change is not part of this interface validation fix.

## Trail interpretation

Five keyword conflicts match the new learning's descriptions of rejected old behavior
against the plan, existing Throwable guards, shared contract checks and regression tests.
They are false positives: staticness is matched to the requirement, and all available
signatures are checked with the shared arity/reference/variance primitives. The builtin
signature-absent path remains intact, as the original Throwable controls verify.
No immutable learning was changed. Overlapping asymmetric-property, DOM test-module and
generics branches were preserved; shared test-module registration may need reconciliation.
